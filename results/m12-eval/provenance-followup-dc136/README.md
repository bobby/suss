The parent dc1364d0 CI foundation job failed because the recorded whole-file
SHA256 for runtime_abi/named_properties.rs preceded its live-field forwarding
changes. Only source-file-sha256 is corrected in abi-adaptations.json. The
guard's hash, algorithm, upstream and license remain unchanged.

commands.json records all sixteen actual exit-0 terminals: the exact first
inventory/scanner step from .github/workflows/ci.yml and the explicit named-property
provenance check. The first command executes 364 Python tests. Input and HEAD
conservation, runner hash and compressed log/input hashes are recorded. This is
local evidence for that step, not a full foundation job, workspace, bootstrap,
integration, issue-completion or promotion result.

The receipt captures source before this README and handoff note are added.
No production code changes follow the tested overlay. Historical live-value
receipts and their verifier remain unchanged and apply to their recorded head.
