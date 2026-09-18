'use strict';

// A leaf reader: every object read is rooted in one full commit ID. It never
// resolves a ref, reads the worktree, or accepts a per-read revision override.
const {execFileSync} = require('node:child_process');
const {ReleaseError, utf8, safePath, uniquePaths} = require('./format');

const FULL_COMMIT = /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/;
function requireCommit(revision) {
  if (typeof revision !== 'string' || !FULL_COMMIT.test(revision))
    throw new ReleaseError('source-invalid', String(revision), 'Expected a full immutable commit ID', 3, 'git-revision');
  return revision;
}
function command(repositoryPath, args) {
  try {
    const env = {...Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_'))),
      GIT_NO_REPLACE_OBJECTS:'1', GIT_OPTIONAL_LOCKS:'0', LC_ALL:'C'};
    return execFileSync('git', ['-C', repositoryPath, ...args],
      {env, maxBuffer:128 * 1024 * 1024, stdio:['ignore','pipe','pipe']});
  } catch (error) {
    throw new ReleaseError('source-unavailable', args.at(-1), `Pinned Git object unreadable: ${error.message}`, 3, 'git-object');
  }
}
function entriesAt(repositoryPath, treeOid, validateModes = true) {
  let entries;
  try {
    entries = utf8(command(repositoryPath, ['ls-tree', '-z', treeOid])).split('\0').filter(Boolean).map(line => {
      const match = /^(\d+) (tree|blob|commit) ([a-f0-9]+)\t([^/]+)$/.exec(line);
      if (!match) throw Error('Malformed Git tree entry');
      return {mode:match[1], type:match[2], oid:match[3], name:match[4]};
    });
    uniquePaths(entries.map(entry => entry.name));
    if (validateModes) for (const entry of entries) {
      if (!((entry.type === 'tree' && entry.mode === '040000') ||
            (entry.type === 'blob' && ['100644','100755'].includes(entry.mode))))
        throw Error(`Unsupported Git mode or type at ${entry.name}`);
    }
  } catch (error) {
    if (error instanceof ReleaseError && error.state === 'source-unavailable') throw error;
    throw new ReleaseError('source-invalid', treeOid, error.message, 3, 'git-tree');
  }
  return entries;
}
function openGitRevision(repositoryPath, revision) {
  requireCommit(revision);
  if (typeof repositoryPath !== 'string' || !repositoryPath)
    throw new ReleaseError('source-unavailable', 'repository', 'Private Git repository is not configured', 3, 'git-object');
  if (command(repositoryPath, ['cat-file', '-t', revision]).toString('ascii').trim() !== 'commit')
    throw new ReleaseError('source-invalid', revision, 'Pinned object is not a commit', 3, 'git-object');
  const rootTree = command(repositoryPath, ['rev-parse', `${revision}^{tree}`]).toString('ascii').trim();
  if (!FULL_COMMIT.test(rootTree)) throw new ReleaseError('source-invalid', revision, 'Invalid commit tree', 3, 'git-object');
  function locate(repoPath) {
    safePath(repoPath);
    let current = {type:'tree', oid:rootTree};
    for (const segment of repoPath.split('/')) {
      if (current.type !== 'tree') throw new ReleaseError('source-invalid', repoPath, 'Expected directory', 3, 'git-tree');
      current = entriesAt(repositoryPath, current.oid, false).find(entry => entry.name === segment);
      if (!current) return null;
      if (!((current.type === 'tree' && current.mode === '040000') ||
            (current.type === 'blob' && ['100644','100755'].includes(current.mode))))
        throw new ReleaseError('source-invalid', repoPath, 'Unsupported Git entry on selected path', 3, 'git-tree');
    }
    return current;
  }
  function readFile(repoPath) {
    const entry = locate(repoPath);
    if (!entry) throw new ReleaseError('source-unavailable', repoPath, 'Pinned file absent', 3, 'git-object');
    if (entry.type !== 'blob') throw new ReleaseError('source-invalid', repoPath, 'Expected regular file', 3, 'git-tree');
    return {path:repoPath, mode:entry.mode === '100644' ? '0644' : '0755',
      bytes:command(repositoryPath, ['cat-file', 'blob', entry.oid])};
  }
  function readTree(repoPath, options = {}) {
    if (Object.keys(options).some(key => key !== 'optional'))
      throw new ReleaseError('source-invalid', repoPath, 'Per-read revision override is forbidden', 3, 'git-revision');
    const root = locate(repoPath);
    if (!root) {
      if (options.optional) return [];
      throw new ReleaseError('source-unavailable', repoPath, 'Pinned tree absent', 3, 'git-object');
    }
    if (root.type !== 'tree') throw new ReleaseError('source-invalid', repoPath, 'Expected directory', 3, 'git-tree');
    const result = [];
    function walk(oid, prefix) {
      const children = entriesAt(repositoryPath, oid);
      if (!children.length) throw new ReleaseError('source-invalid', repoPath, 'Empty Git directory', 3, 'git-tree');
      for (const entry of children) {
        const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
        if (entry.type === 'tree') walk(entry.oid, rel);
        else result.push({path:rel, mode:entry.mode === '100644' ? '0644' : '0755',
          bytes:command(repositoryPath, ['cat-file', 'blob', entry.oid])});
      }
    }
    walk(root.oid, '');
    uniquePaths(result.map(entry => entry.path));
    return result;
  }
  return Object.freeze({revision, readFile, readTree, exists:repoPath => !!locate(repoPath)});
}
module.exports = {requireCommit, openGitRevision};
