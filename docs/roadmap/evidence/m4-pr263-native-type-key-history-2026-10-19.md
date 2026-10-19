# Native type declaration key-history capture

Source base: `ea365554c6d736e0f7dcf509d482b19c05a7598e`.
Independent source-reviewed patch SHA-256:
`21fac7e5ce5395659094a9e2c84e6c76d7044211a82ef51d562e45c7ff6be4ac`.

The nominal analyzer already captures the previous declaration before rebuilding
type metadata, but its subsequent filters and replacement discard earlier key
origins. A new immutable snapshot captures the five explicit parse-type fields
(name, tag, type, num-fields, record) before those destructive steps. It stores
five facts, the actual phase-qualified Global, and only necessary key Form owners.
It retains no predecessor DefinitionInfo, initializer or namespace chain.

Fresh parse-type associations have explicit constructor-site facts. Prior ordinary
name and completed scalar tag have separate literal sites. Original raw metadata
and callable top-fn keys retain native source owners; symbol/string shorthand
uses the reader view's exact constructor discriminant. Existing histories copy
the five facts. Foreign bindings/phases, unknown constructors and unproved legacy
types cannot gain literal provenance. Current type metadata still follows the
existing value policy. Incoming metadata value overrides do not change which
key was first present.

The new fact is captured but is **not yet consumed by Graph**. Other retained
fields and metadata occurrence sharing require further integration. This patch
alone does not repair observable transport or satisfy issue261 acceptance.

Four added native-prepare regressions cover both phases, fresh type/record with
current raw overrides, initially raw keys through repeated type replacements,
completed scalar versus top-fn source tag, shorthand origins, foreign binding/
namespace/phase rejection, and every injected meter error followed by retry.
They are unexecuted additions. Independent review checked the actual fixed-size
representation, source/Global binding, construction sites, before-filter capture,
clone/control/allocation prepayments and cross-file source feasibility. Rustfmt
parsing and cached patch application pass; no native typecheck or runtime pass
is established. Normal bootstrap regeneration remains required.

Primary provenance: ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1,
analyzer.cljc parse-type lines3630–3645. Existing upstream license/provenance
notices apply. Six independently reviewed retained-JS reference cases distinguish
fresh constructors, previous raw keys, current value overrides, repeated type and
record-to-type. Those development observations are not a fresh reference build,
matched native execution or complete callback-order evidence.
