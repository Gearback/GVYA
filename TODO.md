# GVYA TODO

This is the only source-tree location for unfinished project work. Architecture and protocol documents describe current truth only; audit history and delivery reports stay outside the source package.

## Release certification

- [ ] Review/freeze the existing `Cargo.lock` and `package-lock.json` in the connected release environment and confirm `npm run certify:preflight` accepts them unchanged. `bootstrap:locks` is only for an intentional first-time lock regeneration and refuses to overwrite these files.
- [ ] Run the manual GitHub `Release Certification` workflow (or the identical documented commands) and retain the exact certified archive SHA-256.
- [ ] Make a 1.0/freeze-certified claim only after both checkout certification and fresh-extraction certification pass for that exact archive.

