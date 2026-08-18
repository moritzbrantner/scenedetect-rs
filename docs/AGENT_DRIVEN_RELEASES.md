# Agent-driven releases

Destination issue #72 authorizes preparation of exactly one Cargo package:
`scenedetect-core` 0.1.0. It does not authorize the CLI or FFmpeg packages,
behavior changes, source removal from `rust-packages`, or any registry effect
while another release owns shared crates.io capacity.

The release uses two commits to avoid a self-referential source identifier.
The first source commit contains the package source plus reviewed release
controls. A second commit adds only `releases/scenedetect-core-0.1.0.toml`;
`source_sha` identifies the first commit. The package tag must always resolve to
that immutable source commit, never the manifest-only control commit.

This restructuring release deliberately uses structural evidence only:
locked Cargo metadata, exact manifest/dependency/source validation, and one
locked crates.io package archive for `scenedetect-core`. It does not run or
claim unit, parity, workspace, Clippy, documentation, consumer, build, or broad
package-suite evidence. The reduced evidence does not reduce the irreversible
operation safeguards.

Publication remains fail-closed until the open destination issue contains the
exact current control SHA and manifest SHA-256 and carries `release:approved`.
The Agent Loop master owns that short-lived approval and the receipt-gated
invocation. Preparation workers never approve, publish, merge, tag, create a
GitHub Release, or close the issue.

The publisher is idempotent. It packages the one selected crate, compares its
archive checksum with any existing non-yanked crates.io version, uploads only
an absent version, and refuses an incompatible or yanked artifact. It creates
or accepts the declared tag only when it resolves to `source_sha`, and creates
or accepts the matching GitHub Release only after the registry artifact and
remote tag are exact. Partial failure stops immediately and a later invocation
repeats the same validations before resuming.

Credentials remain in their normal Cargo/GitHub stores and must never be copied
into repository files or logs.
