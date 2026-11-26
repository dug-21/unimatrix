"use strict";
// Skip option for tests of the Linux-only local transport (nan-024 D-032:
// macOS/Windows are cloud-only). Spread/pass as the options arg of it()/test().
const linuxLocal = process.platform === "linux" ? {} : { skip: "D-032: local transport is Linux-only (macOS/Windows are cloud-only)" };
module.exports = { linuxLocal };
