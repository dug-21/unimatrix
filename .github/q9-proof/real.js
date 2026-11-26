'use strict';
// Q9 follow-up (nan-024 D-072/D-073): same proof as driver.js but against the REAL installer output.
// init writes the shim/entry into a fixture HOME containing a space; this reads the WRITTEN Windows MCP
// entry and hook command back and spawns them with Claude-style (Node, shell:false) and Codex-style
// (Rust Command .args / .raw_arg, `cmd.exe /C` with and without the outer-quote wrapper) semantics, and Git Bash.
// Usage: node real.js   (needs E2E_CERT_DIR, RUNNER_TEMP/spawn.exe built by the workflow)
const fs = require('fs'), path = require('path');
const H = require('../../packages/unimatrix/test/helpers/e2e-harness.js');
const spawnExe = path.join(process.env.RUNNER_TEMP || '.', 'spawn.exe');
const norm = (p) => String(p).replace(/\\/g, '/').toLowerCase();
const results = [];
const check = (name, ok, detail) => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} | ${name}${ok ? '' : '\n     ' + detail}`); };

(async () => {
  const cert = H.loadCert();
  if (!cert) { console.error('no cert (E2E_CERT_DIR)'); process.exit(2); }
  const stub = await H.startStub(cert);
  const f = H.makeFixture();
  const env = H.baseEnv(f, cert, { CODEX_HOME: f.codex });
  console.log(`home=${f.home} node=${process.execPath}`);
  const init = await H.runInit(f, env, ['--bundle', H.bundleFor(stub, cert)]);
  check('init (cloud attach) exit 0', init.code === 0, init.out + init.err);

  // D-072: MCP entry is the D-047 fallback form, exactly.
  const cj = JSON.parse(fs.readFileSync(path.join(f.home, '.claude.json'), 'utf8'));
  const entry = cj.projects[Object.keys(cj.projects)[0]].mcpServers.unimatrix;
  const inst = JSON.parse(fs.readFileSync(path.join(f.home, '.unimatrix', 'install.json'), 'utf8'));
  console.log('written MCP entry: ' + JSON.stringify(entry));
  check('D-047 form: command == recorded node', norm(entry.command) === norm(inst.node), JSON.stringify(entry) + ' node=' + inst.node);
  check('D-047 form: args = [<home>/.unimatrix/bin/shim-entry.js, mcp-stdio]',
    entry.args.length === 2 && norm(entry.args[0]) === norm(path.join(f.home, '.unimatrix', 'bin', 'shim-entry.js')) && entry.args[1] === 'mcp-stdio', JSON.stringify(entry.args));
  check('D-047 form: no cmd wrapper', !/cmd(\.exe)?$/i.test(entry.command) && !entry.args.some((a) => /\.cmd|\/c/i.test(a)), JSON.stringify(entry));

  const msg = JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'q9', version: '0' } } }) + '\n';
  const mcpEnv = Object.assign({}, env, entry.env || {});
  let r = await H.mcpInitialize(entry, f.proj, env);
  check('MCP entry via Node spawn(shell:false) [Claude Code]: initialize result', r.out.startsWith('{') && !!JSON.parse(r.out.split('\n')[0]).result, r.out + r.err);
  r = await H.run(spawnExe, ['args', entry.command].concat(entry.args), { spawn: { cwd: f.proj, env: mcpEnv }, input: msg });
  check('MCP entry via Rust Command.args [Codex]: initialize result', r.out.startsWith('{') && !!JSON.parse(r.out.split('\n')[0]).result, r.out + r.err);

  // D-073: the written .cmd hook command through cmd.exe /C (raw) and Git Bash.
  const comspec = process.env.ComSpec || 'cmd.exe';
  const files = [['claude settings.local.json', path.join(f.proj, '.claude', 'settings.local.json')], ['codex hooks.json', path.join(f.codex, 'hooks.json')]];
  let n = 0;
  for (const [label, file] of files) {
    const line = H.hookCommand(file, 'PostToolUse');
    console.log(`${label} hook command: ${line}`);
    const forms = [
      ['cmd.exe /C <line> Rust raw_arg [Codex]', () => [spawnExe, ['raw', comspec, '/C', line]]],
      ['cmd.exe /C "<line>" Rust raw_arg [Codex wrapping]', () => [spawnExe, ['raw', comspec, '/C', '"' + line + '"']]],
      ['cmd.exe /C <line> Node verbatim', () => [comspec, ['/C', line], true]],
      ['cmd.exe /C "<line>" Node verbatim', () => [comspec, ['/C', '"' + line + '"'], true]],
    ];
    const gb = H.findBash();
    if (gb) { forms.push(['Git Bash -c [Claude Code]', () => [gb, ['-c', line]]]); forms.push(['Git Bash -lc', () => [gb, ['-lc', line]]]); }
    for (const [name, mk] of forms) {
      const sid = 'q9-real-' + n++;
      const [file_, args, verbatim] = mk();
      const before = stub.events().length;
      const o = await H.run(file_, args, { spawn: { cwd: f.proj, env, windowsVerbatimArguments: !!verbatim }, input: H.event(f, sid) });
      const got = stub.events().slice(before).filter((e) => e.body.includes(sid));
      check(`${label} | ${name}: exit 0, empty stdout, observation arrived`, o.code === 0 && o.out === '' && got.length >= 1, `exit=${o.code} out=${JSON.stringify(o.out)} err=${JSON.stringify(o.err.slice(0, 300))} events=${got.length}`);
    }
  }
  await stub.close();
  f.cleanup();
  const ok = results.every(Boolean);
  console.log(`RESULT real: ${ok ? 'PASS' : 'FAIL'}`);
  process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e && e.stack || e); process.exit(1); });
