# C1 Token Table — `http/token_table.rs` (new)

ADR-001 (#5814). Target < 400 lines; tests in `http/token_table/tests.rs` (sibling, `#[cfg(test)] mod tests;`).

## Purpose
Persistent scoped-token store `{data_dir}/tokens.json` (0600). Read side: per-request stat-stamp snapshot for C2. Write side: locked atomic mint/revoke for C7. Tombstones never deleted.

## Constants / Types

```
const TOKENS_FILE = "tokens.json"
const TOKENS_TMP  = "tokens.json.tmp"
const TOKENS_LOCK = "tokens.lock"
const FILE_VERSION: u32 = 1
LABEL_RE = ^[a-z0-9][a-z0-9-]{0,62}$     // hand-rolled check, same style as ProjectSlug (no regex crate needed)

#[derive(Serialize, Deserialize)] #[serde(deny_unknown_fields)]
struct TokenFile   { v: u32, tokens: Vec<TokenRecord> }
#[derive(Serialize, Deserialize, Clone)] #[serde(deny_unknown_fields)]
struct TokenRecord { label: String, sha256: String, mode: ModeWire, allowed_slugs: Vec<String>,
                     created_at: u64, revoked_at: Option<u64> }   // revoked_at serializes as null when None (no skip_serializing_if)
#[serde(rename_all = "kebab-case")] enum ModeWire { Read, ReadWrite }   // "read" | "read-write"; maps 1:1 to principal::TokenMode

pub(crate) struct ScopedEntry { label: String, mode: TokenMode, allowed_slugs: Vec<ProjectSlug>, revoked: bool }
struct ParsedEntry { hash: [u8; 32], entry: ScopedEntry }
pub(crate) struct EntryView { label, mode: TokenMode, allowed_slugs: Vec<ProjectSlug>, created_at: u64, revoked_at: Option<u64> }

#[derive(PartialEq, Clone, Copy)] struct Stamp { mtime_ns: i128, len: u64, inode: u64 }
enum SnapshotState { Missing, Loaded(Vec<ParsedEntry>), Corrupt }
struct Snapshot { stamp: Option<Stamp>, state: SnapshotState }

pub(crate) struct TokenTable { data_dir: PathBuf, snap: RwLock<Snapshot> }
```

## Parse (strict) — `fn parse_file(bytes: &[u8]) -> Result<Vec<ParsedEntry>, ParseError>`
```
file = serde_json::from_slice::<TokenFile>(bytes)?          // unknown key anywhere -> Err
if file.v != 1 -> Err
seen = HashSet<String>                                      // label set (NOT keyed by hash; R-06)
for rec in file.tokens:
    if !is_label(rec.label) -> Err
    if !seen.insert(rec.label) -> Err(duplicate label)
    if rec.allowed_slugs.is_empty() -> Err
    slugs = rec.allowed_slugs.map(ProjectSlug::try_from)?     // any invalid slug -> Err
    hash = decode_lower_hex_32(rec.sha256)?                   // exactly 64 lowercase hex
    push ParsedEntry{hash, entry: ScopedEntry{label, mode, slugs, revoked: rec.revoked_at.is_some()}}
Ok(entries)
```
ParseError carries only a reason string (never the hash value).

## Read Path

### `pub(crate) fn open(data_dir: &Path) -> Self`
```
TokenTable{ data_dir, snap: RwLock::new(Snapshot{stamp: None, state: Missing}) }
then self.refresh()   // initial load; never fails (corrupt -> Corrupt state, logged)
```

### `fn stat_stamp(&self) -> Result<Option<Stamp>, io::Error>`
```
match fs::metadata(data_dir/TOKENS_FILE):
  Ok(m)  -> Some(Stamp{ mtime_ns: m.mtime()*1e9 + m.mtime_nsec(), len: m.len(), inode: m.ino() })  // std::os::unix::fs::MetadataExt
  Err(NotFound) -> None
  Err(e) -> Err(e)
```

### `fn refresh(&self)`
```
stamp = match stat_stamp():
    Err(e) -> { write-lock; snap = {stamp: None, state: Corrupt}; tracing::error!(error=%e, "tokens.json stat failed; scoped tokens fail closed"); return }
    Ok(s) -> s
if read-lock.stamp == stamp && state is not (Corrupt with stamp None): return      // fast path
write-lock:
  if snap.stamp == stamp: return                     // another thread reloaded
  match stamp:
    None -> snap = {None, Missing}
    Some(st) -> match fs::read(path).and_then(parse_file):
        Ok(entries) -> snap = {Some(st), Loaded(entries)}
        Err(e)      -> snap = {Some(st), Corrupt}; tracing::error!(reason=%e, "tokens.json invalid; scoped tokens fail closed")  // once per stamp: only on stamp change
```
Never retains a previous `Loaded` across a stamp change (no last-good; R-05).
Note: rename gives a new inode, so equal mtime+len writes are still detected (R-05.3).

Loop body lives in private `scan(&self, presented, visits: &mut usize)`; `lookup_ct` calls `scan(p, &mut 0)`; `#[cfg(test)] lookup_ct_counted` exposes the count. `atomic_write` calls `#[cfg(test)] AFTER_TMP_CREATE` after tmp create. See OVERVIEW §Test-Only Seams.

### `pub(crate) fn lookup_ct(&self, presented: &[u8; 32]) -> Option<ScopedEntry>`
```
self.refresh()
digest: [u8;32] = Sha256::digest(presented)
read-lock
entries = match state { Loaded(e) -> e, Missing | Corrupt -> return None }
found = Choice::from(0); idx = 0u64 (constant-time select)
for (i, pe) in entries.iter().enumerate():           // NO break, NO early return
    eq = pe.hash.ct_eq(&digest)
    idx = u64::conditional_select(&idx, &(i as u64), eq)
    found |= eq
if !bool::from(found): return None
e = entries[idx].entry.clone()
if e.revoked: return None      // decided after full scan; caller maps to identical 401
Some(e)
```

## Write Path (C7 only; server never writes)

### `fn with_lock<T>(&self, f: impl FnOnce(TokenFile) -> Result<(Option<TokenFile>, T), ServerError>) -> Result<T, ServerError>`
```
lock = OpenOptions::new().create(true).write(true).mode(0o600).open(data_dir/TOKENS_LOCK)?
fs2::FileExt::lock_exclusive(&lock)?                   // fs2 = "0.4" already a dep (vnc-004 #189)
current = match fs::read(TOKENS_FILE):
    NotFound -> TokenFile{v:1, tokens: []}
    Ok(b) -> serde_json::from_slice::<TokenFile>(b) + parse_file(b) validation
             -> on Err: return ServerError::Config("tokens.json is invalid; fix or restore it before minting/revoking")   // never rewrite a corrupt file
(new_file, out) = f(current)?
if let Some(nf) = new_file: atomic_write(nf)?
// lock released on drop
Ok(out)
```

### `fn atomic_write(&self, file: &TokenFile) -> Result<(), ServerError>`
```
bytes = serde_json::to_vec_pretty(file)? + b"\n"
tmp = data_dir/TOKENS_TMP
fs::remove_file(tmp) ignoring NotFound               // stale tmp from crash
f = OpenOptions::new().write(true).create_new(true).mode(0o600).open(tmp)?
f.write_all(bytes); f.sync_all()
fs::rename(tmp, data_dir/TOKENS_FILE)?
File::open(data_dir).sync_all()                      // dir fsync, best effort (log warn on Err)
on any Err after create: remove tmp (best effort), return Err
```

### `pub(crate) fn mint(&self, label: &str, mode: TokenMode, slug: &ProjectSlug) -> Result<[u8; 32], ServerError>`
```
if !is_label(label) -> Err(InvalidInput{field:"label", reason:"must match ^[a-z0-9][a-z0-9-]{0,62}$"})
secret = [0u8;32]; rand::rngs::OsRng.try_fill_bytes(&mut secret)?   // OS CSPRNG (rand 0.9 present)
with_lock(|mut file|
    if file.tokens.any(|r| r.label == label) -> Err(InvalidInput{field:"label", reason:"label already exists (labels are never reusable)"})
    file.tokens.push(TokenRecord{ label, sha256: lower_hex(Sha256(secret)), mode, allowed_slugs:[slug], created_at: now_secs(), revoked_at: None })
    Ok((Some(file), ()))
)?
Ok(secret)
```

### `pub(crate) fn revoke(&self, label: &str, slug: &ProjectSlug) -> Result<(), ServerError>`
```
with_lock(|mut file|
    rec = file.tokens.find_mut(label) else Err(InvalidInput{"label", "no token with that label"})
    if !rec.allowed_slugs.contains(slug) -> Err(same "no token with that label for this project")   // not reveal other slugs
    if rec.revoked_at.is_some() -> Err(InvalidInput{"label", "token already revoked"})
    rec.revoked_at = Some(now_secs())
    Ok((Some(file), ()))
)
```
Error paths return `None` file → no write → file byte-identical (R-05.5).

### `pub(crate) fn list_for(&self, slug: &ProjectSlug) -> Vec<EntryView>`
Signature per architecture returns `Vec`. Reads file directly (not snapshot), no lock needed (rename is atomic).
```
match fs::read + parse:
  NotFound -> vec![]
  Err      -> vec![] AND tracing::error  // C7 uses `list_for_checked` (below) to surface corruption
filter records whose allowed_slugs contains slug; map to EntryView (no hash)
```
Gap flagged: architecture's `list_for -> Vec<EntryView>` cannot report a corrupt file. Pseudocode adds `pub(crate) fn list_for_checked(&self, slug) -> Result<Vec<EntryView>, ServerError>`; `list_for` delegates and returns empty on Err. C7 calls `list_for_checked`.

## Error Handling
- Server read path never returns errors: Missing → empty, Corrupt/stat error → scoped fail-closed (None).
- Writers: `ServerError::InvalidInput` (label/duplicate/unknown/revoked), `ServerError::Config` (corrupt file, I/O). Messages never include secret or hash.

## Key Test Scenarios
- Parse: unknown key (top and record), duplicate label, empty slugs, bad slug, uppercase hex, 63-char vs 64-char label, leading `-` → reject whole file.
- `lookup_ct`: match at index 0 and n-1; tombstone match → None; no `HashMap` keyed by hash, no `break` (static review, R-06).
- Stamp: rewrite with equal len inside one mtime tick still reloads (inode); delete file → Missing; corrupt → None for all, then fix → recovers; revoked never revived.
- Writers: 20 parallel mints (threads/processes) all present; tmp and file 0600; no tmp after success; re-mint revoked label fails; revoke unknown/other-slug/already-revoked → file byte-identical; later mint preserves tombstone.
