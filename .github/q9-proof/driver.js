'use strict';
// Q9 (nan-024 D-034) proof driver. Stubs stand in for the real shim (not yet written).
// Usage: node driver.js <form>   form: mcp-primary | mcp-fallback | hook
const fs = require('fs'), path = require('path'), cp = require('child_process');
const form = process.argv[2];
const root = path.join(process.env.RUNNER_TEMP || require('os').tmpdir(), 'q9 home dir');
const bin = path.join(root, '.unimatrix', 'bin');
fs.mkdirSync(bin, { recursive: true });
fs.writeFileSync(path.join(bin, 'shim-entry.js'), `
const a = process.argv.slice(2); let d = '';
process.stdin.on('data', c => d += c);
process.stdin.on('end', () => {
  process.stdout.write('Q9MARKER args=' + JSON.stringify(a) + ' stdin=' + d.trim() + '\\n');
  process.exit(Number(process.env.Q9_EXIT || 0));
});
`);
fs.writeFileSync(path.join(bin, 'unimatrix.cmd'),
  `@echo off\r\n"${process.execPath}" "%~dp0shim-entry.js" %*\r\nexit /b %errorlevel%\r\n`);
const env = { ...process.env, USERPROFILE: root, HOME: root, Q9_EXIT: '7' };
const PAYLOAD = 'payload-from-host';
const cases = [];
function add(name, fn) { cases.push([name, fn]); }

function run(file, args, opts) {
  return new Promise(res => {
    let out = '', err = '', t;
    let p;
    try { p = cp.spawn(file, args, { env, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true, ...opts }); }
    catch (e) { return res({ code: null, out, err: String(e) }); }
    p.stdout.on('data', c => out += c); p.stderr.on('data', c => err += c);
    p.on('error', e => { err += String(e); });
    p.on('close', code => { clearTimeout(t); res({ code, out, err }); });
    p.stdin.on('error', () => {});
    p.stdin.end(PAYLOAD + '\n');
    t = setTimeout(() => { try { p.kill(); } catch (_) {} }, 20000);
  });
}
function verify(r, wantArgs) {
  const ok = r.out.includes('Q9MARKER') && r.out.includes('stdin=' + PAYLOAD) &&
    r.out.includes('args=' + JSON.stringify(wantArgs)) && r.code === 7;
  return { ok, detail: `exit=${r.code} out=${JSON.stringify(r.out.trim())} err=${JSON.stringify(r.err.trim().slice(0, 300))}` };
}

if (form === 'mcp-primary') {
  const argv = ['/d', '/s', '/c', '""%USERPROFILE%\\.unimatrix\\bin\\unimatrix.cmd" mcp-stdio"'];
  // Claude Code: Node child_process.spawn(command, args), shell:false (default arg quoting)
  add('node-spawn(cmd, argv) [Claude Code semantics]', () => run('cmd', argv, {}));
  // Rust std::process::Command .args() (Codex): driven by compiled harness
  add('rust Command.args(argv) [Codex semantics]', () => run(path.join(process.env.RUNNER_TEMP || '.', 'spawn.exe'), ['args', 'cmd', ...argv], {}));
  var want = ['mcp-stdio'];
} else if (form === 'mcp-fallback') {
  const argv = [bin.replace(/\\/g, '/') + '/shim-entry.js', 'mcp-stdio'];
  add('node-spawn(node, argv) [Claude Code semantics]', () => run(process.execPath, argv, {}));
  add('rust Command.args(node, argv) [Codex semantics]', () => run(path.join(process.env.RUNNER_TEMP || '.', 'spawn.exe'), ['args', process.execPath, ...argv], {}));
  var want = ['mcp-stdio'];
} else if (form === 'hook') {
  const shim = bin.replace(/\\/g, '/') + '/unimatrix.cmd';
  const cmdline = `"${shim}" hook SessionStart`;
  var want = ['hook', 'SessionStart'];
  add('cmd.exe /C <string> via Node default quoting', () => run('cmd.exe', ['/C', cmdline], {}));
  add('cmd.exe /C <string> verbatim (Node windowsVerbatimArguments)', () => run('cmd.exe', ['/C', cmdline], { windowsVerbatimArguments: true }));
  add('cmd.exe /C <string> Rust raw_arg [Codex semantics]', () => run(path.join(process.env.RUNNER_TEMP || '.', 'spawn.exe'), ['raw', 'cmd.exe', '/C', cmdline], {}));
  const gb = process.env.GIT_BASH || 'C:\\Program Files\\Git\\bin\\bash.exe';
  add('Git Bash -c <string> [Claude Code semantics]', () => run(gb, ['-c', cmdline], {}));
  add('Git Bash -lc <string>', () => run(gb, ['-lc', cmdline], {}));
} else { console.error('bad form'); process.exit(2); }

(async () => {
  console.log(`form=${form} USERPROFILE=${root} node=${process.execPath}`);
  let all = true;
  for (const [name, fn] of cases) {
    const v = verify(await fn(), want);
    // Informational: Node default quoting escapes the inner quotes; no real harness spawns this way
    // (Claude Code uses Git Bash, Codex uses raw_arg), so it does not gate the form (D-073/D-076).
    const info = /Node default quoting/.test(name);
    all = all && (v.ok || info);
    console.log(`${v.ok ? 'PASS' : (info ? 'INFO(not gating)' : 'FAIL')} | ${form} | ${name}\n     ${v.detail}`);
  }
  console.log(`RESULT ${form}: ${all ? 'PASS' : 'FAIL'}`);
  process.exit(all ? 0 : 1);
})();
