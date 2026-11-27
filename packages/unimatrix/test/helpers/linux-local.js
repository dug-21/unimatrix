"use strict";
// Platform skip options (spread/pass as the options arg of it()/test()/describe()).
// linuxLocal: tests of the Linux-only local transport. Personal-cloud goal platform rule
// (#5957): local mode is Linux-only; macOS/Windows are cloud-only (shared wiring there is
// covered via `init --bundle`, D-042).
const WIN = process.platform === "win32";
const linuxLocal = process.platform === "linux" ? {} : { skip: "personal-cloud goal (#5957): local mode is Linux-only; macOS/Windows are cloud-only" };
// win32 symlink creation needs SeCreateSymbolicLinkPrivilege (EPERM otherwise).
const winSymlink = WIN ? { skip: "win32: creating symlinks needs SeCreateSymbolicLinkPrivilege (EPERM)" } : {};
// win32 has no POSIX mode bits (chmod only toggles read-only; store protection is ACLs).
const winModes = WIN ? { skip: "win32: no POSIX mode bits (chmod toggles read-only only; store uses ACLs)" } : {};
/** First skip among option objects, or {}. */
const anySkip = (...os) => os.find((o) => o && o.skip) || {};
module.exports = { linuxLocal, winSymlink, winModes, anySkip };
