# Optional verification profile

This draft profile records the deliberately structural release evidence for
issue #72. It does not activate an additional worker gate and it does not turn
the omitted behavioral suites into passing evidence.

The full tier contains only locked Cargo metadata, exact one-package release
contract validation, and a locked crates.io package archive. Clean exact-head,
destination issue approval, checksum/yank, and immutable source-tag checks are
enforced by the publisher immediately around irreversible effects.
