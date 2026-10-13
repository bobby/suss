// Copyright (c) Rich Hickey. All rights reserved.
// SPDX-License-Identifier: EPL-1.0
// Complete pinned upstream sources/notices remain in
// docs/compatibility/record-macro-foundations. These helpers execute compiled
// Suss; no Rust form evaluator builds record expansions.
pub(crate) const SOURCE: &str = r####"
;; Copyright (c) Rich Hickey. All rights reserved.
;; The use and distribution terms for this software are covered by the
;; Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php).
;; By using this software in any fashion, you agree to be bound by its terms.
;; You must not remove this notice, or any other, from this software.
;; SPDX-License-Identifier: EPL-1.0
;;
;; Integrated compiled adaptation of the complete pinned collect-protocols and
;; prepare-protocol-masks forms, plus fast-path-protocols and its partition
;; count. Whole forms and pin/license hashes remain in:
;; docs/compatibility/record-macro-foundations/{collect-protocols,prepare-protocol-masks}.cljc
;; docs/compatibility/record-analyzer-context/{core-fast-path-protocols-813,core-fast-path-protocol-partitions-count-831}.cljc
;; Provenance IDs: macro:collect-protocols:1754,
;; macro:prepare-protocol-masks:1710, macro:fast-path-protocols:813,
;; macro:fast-path-protocol-partitions-count:831.
;; Installed in the Macro Store; phase/scope and restoration checks pass.
;; These checks do not certify complete public record or M4 acceptance.
(ns suss.internal.record-protocol-facts)

(def ^:private record-facts-error
  (fn [message detail]
    (throw (suss.core/ex-info message detail))))

(def ^:private canonical-namespace
  (fn [namespace-name]
    (if (= namespace-name "cljs.core") "suss.core" namespace-name)))

(def ^:private canonical-global
  (fn [global]
    (let [namespace-name (namespace global)]
      (if (= namespace-name "cljs.core")
        (symbol "suss.core" (name global))
        global))))

;; Retain the complete pinned symbol order, including the repeated
;; IPrintWithWriter. The source zipmap overwrites an earlier duplicate with the
;; later [partition bit]; fast-path-entry below reproduces that last-wins rule
;; without constructing a startup hash table.
(def ^:private fast-path-names
  '[IFn ICounted IEmptyableCollection ICollection IIndexed ASeq ISeq INext
    ILookup IAssociative IMap IMapEntry ISet IStack IVector IDeref
    IDerefWithTimeout IMeta IWithMeta IReduce IKVReduce IEquiv IHash
    ISeqable ISequential IList IRecord IReversible ISorted IPrintWithWriter IWriter
    IPrintWithWriter IPending IWatchable IEditableCollection ITransientCollection
    ITransientAssociative ITransientMap ITransientVector ITransientSet
    IMultiFn IChunkedSeq IChunkedNext IComparable INamed ICloneable IAtom
    IReset ISwap IIterable IDrop])

(def ^:private fast-path-rows (quote [[cljs.core/IFn 0 1] [cljs.core/ICounted 0 2] [cljs.core/IEmptyableCollection 0 4] [cljs.core/ICollection 0 8] [cljs.core/IIndexed 0 16] [cljs.core/ASeq 0 32] [cljs.core/ISeq 0 64] [cljs.core/INext 0 128] [cljs.core/ILookup 0 256] [cljs.core/IAssociative 0 512] [cljs.core/IMap 0 1024] [cljs.core/IMapEntry 0 2048] [cljs.core/ISet 0 4096] [cljs.core/IStack 0 8192] [cljs.core/IVector 0 16384] [cljs.core/IDeref 0 32768] [cljs.core/IDerefWithTimeout 0 65536] [cljs.core/IMeta 0 131072] [cljs.core/IWithMeta 0 262144] [cljs.core/IReduce 0 524288] [cljs.core/IKVReduce 0 1048576] [cljs.core/IEquiv 0 2097152] [cljs.core/IHash 0 4194304] [cljs.core/ISeqable 0 8388608] [cljs.core/ISequential 0 16777216] [cljs.core/IList 0 33554432] [cljs.core/IRecord 0 67108864] [cljs.core/IReversible 0 134217728] [cljs.core/ISorted 0 268435456] [cljs.core/IPrintWithWriter 0 536870912] [cljs.core/IWriter 0 1073741824] [cljs.core/IPrintWithWriter 0 2147483648] [cljs.core/IPending 1 1] [cljs.core/IWatchable 1 2] [cljs.core/IEditableCollection 1 4] [cljs.core/ITransientCollection 1 8] [cljs.core/ITransientAssociative 1 16] [cljs.core/ITransientMap 1 32] [cljs.core/ITransientVector 1 64] [cljs.core/ITransientSet 1 128] [cljs.core/IMultiFn 1 256] [cljs.core/IChunkedSeq 1 512] [cljs.core/IChunkedNext 1 1024] [cljs.core/IComparable 1 2048] [cljs.core/INamed 1 4096] [cljs.core/ICloneable 1 8192] [cljs.core/IAtom 1 16384] [cljs.core/IReset 1 32768] [cljs.core/ISwap 1 65536] [cljs.core/IIterable 1 131072] [cljs.core/IDrop 1 262144]]))

(def ^:private linear-contains?
  (fn [values target]
    (loop [remaining values]
      (if (seq remaining)
        (if (= (first remaining) target)
          true
          (recur (next remaining)))
        false))))

(def ^:private fast-path-index [["ASeq" (nth fast-path-rows 5)] ["IAssociative" (nth fast-path-rows 9)] ["IAtom" (nth fast-path-rows 46)] ["IChunkedNext" (nth fast-path-rows 42)] ["IChunkedSeq" (nth fast-path-rows 41)] ["ICloneable" (nth fast-path-rows 45)] ["ICollection" (nth fast-path-rows 3)] ["IComparable" (nth fast-path-rows 43)] ["ICounted" (nth fast-path-rows 1)] ["IDeref" (nth fast-path-rows 15)] ["IDerefWithTimeout" (nth fast-path-rows 16)] ["IDrop" (nth fast-path-rows 50)] ["IEditableCollection" (nth fast-path-rows 34)] ["IEmptyableCollection" (nth fast-path-rows 2)] ["IEquiv" (nth fast-path-rows 21)] ["IFn" (nth fast-path-rows 0)] ["IHash" (nth fast-path-rows 22)] ["IIndexed" (nth fast-path-rows 4)] ["IIterable" (nth fast-path-rows 49)] ["IKVReduce" (nth fast-path-rows 20)] ["IList" (nth fast-path-rows 25)] ["ILookup" (nth fast-path-rows 8)] ["IMap" (nth fast-path-rows 10)] ["IMapEntry" (nth fast-path-rows 11)] ["IMeta" (nth fast-path-rows 17)] ["IMultiFn" (nth fast-path-rows 40)] ["INamed" (nth fast-path-rows 44)] ["INext" (nth fast-path-rows 7)] ["IPending" (nth fast-path-rows 32)] ["IPrintWithWriter" (nth fast-path-rows 31)] ["IRecord" (nth fast-path-rows 26)] ["IReduce" (nth fast-path-rows 19)] ["IReset" (nth fast-path-rows 47)] ["IReversible" (nth fast-path-rows 27)] ["ISeq" (nth fast-path-rows 6)] ["ISeqable" (nth fast-path-rows 23)] ["ISequential" (nth fast-path-rows 24)] ["ISet" (nth fast-path-rows 12)] ["ISorted" (nth fast-path-rows 28)] ["IStack" (nth fast-path-rows 13)] ["ISwap" (nth fast-path-rows 48)] ["ITransientAssociative" (nth fast-path-rows 36)] ["ITransientCollection" (nth fast-path-rows 35)] ["ITransientMap" (nth fast-path-rows 37)] ["ITransientSet" (nth fast-path-rows 39)] ["ITransientVector" (nth fast-path-rows 38)] ["IVector" (nth fast-path-rows 14)] ["IWatchable" (nth fast-path-rows 33)] ["IWithMeta" (nth fast-path-rows 18)] ["IWriter" (nth fast-path-rows 30)]])

(def ^:private fast-path-entry
  (fn [global]
    (let [global (canonical-global global)]
      (if (= (namespace global) "suss.core")
        (let [target (name global)]
          (loop [lo 0 hi (count fast-path-index)]
            (if (< lo hi)
              (let [mid (quot (+ lo hi) 2)
                    entry (nth fast-path-index mid)
                    order (compare target (first entry))]
                (if (zero? order) (second entry)
                  (if (< order 0) (recur lo mid) (recur (inc mid) hi))))
              nil)))
        nil))))

(def ^:private fast-path-unique-count (count fast-path-index))

(def ^:private fast-path-partitions-count
  ;; The pinned whole form uses the imported core mod/quot implementation.
  ;; fast-path-unique-count is a value binding, just as count is in that form.
  (let [c fast-path-unique-count
        m (mod c 32)]
    (if (zero? m)
      (quot c 32)
      (inc (quot c 32)))))

(def ^:private compiler-state
  (fn [env]
    (let [caller (get env :ns)
          live (get env :suss/catalog)
          compiler (get env :suss/compiler-catalog)
          phase (get compiler :phase)
          current (get compiler :current)
          namespaces (get compiler :namespaces)
          signatures (get compiler :protocol-signatures)]
      (if (and caller live compiler phase current namespaces signatures
               (= (get caller :name) current)
               (= (get live :name) current))
        [caller live compiler phase current namespaces signatures]
        (record-facts-error
          "Record protocol resolution requires the live, phase-matched compiler catalog"
          {:phase phase :current current :caller (get caller :name)
           :live (get live :name)})))))

(def ^:private namespace-facts
  (fn [live current namespaces namespace-name]
    (let [key (symbol namespace-name)]
      (if (= key current)
        live
        (get namespaces key)))))

(def ^:private native-object-global
  (fn [compiler phase]
    ;; This is an explicit required compiler projection, not a synthetic js
    ;; namespace or var record. See the notes for the typed producer contract.
    (let [fact (get (get compiler :native-interfaces) (symbol "js" "Object"))]
      (if (and fact
               (= (get fact :global) (symbol "js" "Object"))
               (= (get fact :phase) phase)
               (= (get fact :binding-kind) :environment-core)
               (= (get fact :export) "object-constructor")
               (= (get fact :extension-kind) :native-object))
        (symbol "js" "Object")
        nil))))

(def ^:private binding-present?
  (fn [live current namespaces signatures global]
    (let [global (canonical-global global)]
      (if (contains? signatures global)
        true
        (let [namespace-name (namespace global)
              facts (namespace-facts live current namespaces namespace-name)
              definitions (or (get facts :suss/binding-names) (get facts :defs))]
          (and definitions
               (contains? definitions (symbol (name global)))))))))

(def ^:private require-binding
  (fn [live current namespaces signatures global original]
    (let [global (canonical-global global)]
      (if (binding-present? live current namespaces signatures global)
        global
        (record-facts-error
          "Record protocol symbol does not resolve to an actual caller-phase binding"
          {:symbol original :resolved global})))))

(def ^:private make-protocol-resolver
  (fn [state]
    (let [live (nth state 1)
          compiler (nth state 2)
          phase (nth state 3)
          current (nth state 4)
          namespaces (nth state 5)
          signatures (nth state 6)
          ;; Like Environment::resolve, use the actual live Environment scope.
          ;; The serialized :ns is the macro caller snapshot and is checked for
          ;; identity above, but can predate provisional namespace revisions.
          source-namespace (get live :name)
          aliases (get live :requires)
          refers (get live :suss/refers)
          exclusions (get live :excludes)
          own-namespace (canonical-namespace (name source-namespace))
          ;; Extract actual definition maps once from the same catalog snapshot.
          ;; Signature membership still admits provisional protocol declarations.
          own-facts (namespace-facts live current namespaces own-namespace)
          own-definitions (or (get own-facts :suss/binding-names) (get own-facts :defs))
          core-facts (namespace-facts live current namespaces "suss.core")
          core-definitions (or (get core-facts :suss/binding-names) (get core-facts :defs))
          ;; Experiment: faithful namespace/local -> global projection from
          ;; actual phase-filtered Environment.protocols, never metadata :name.
          signature-index (get compiler :protocol-globals-by-namespace)
          own-signatures (get signature-index own-namespace)
          core-signatures (get signature-index "suss.core")
          resolved-bindings (get compiler :unqualified-protocol-bindings)]
      (when-not (and (map? signature-index) (map? resolved-bindings))
        (record-facts-error "Record protocol resolution requires its actual protocol namespace index"
          {:phase phase :current current}))
      (fn [protocol-symbol]
        (let [symbol-namespace (namespace protocol-symbol)
          symbol-name (name protocol-symbol)]
      ;; Compiler HIR recognizes this exact source spelling before ordinary
      ;; named protocol resolution. Require its genuine typed projection.
      (if (and (nil? symbol-namespace) (= symbol-name "Object"))
        (let [global (native-object-global compiler phase)]
          (if global
            global
            (record-facts-error
              "Native Object extension requires its genuine typed compiler fact"
              {:symbol protocol-symbol :phase phase :current current})))
        (if symbol-namespace
        (let [target-namespace
              ;; Source-qualified clojure.core is reserved before aliases;
              ;; canonical-global still preserves genuine clojure.core globals.
              (cond
                (= symbol-namespace "clojure.core") "suss.core"
                (or (= symbol-namespace "suss.core")
                    (= symbol-namespace "cljs.core")
                    (= symbol-namespace "js"))
                (canonical-namespace symbol-namespace)
                :else
                (let [alias-target (get aliases (symbol symbol-namespace))]
                  (canonical-namespace
                    (if alias-target (name alias-target) symbol-namespace))))
              target (symbol target-namespace symbol-name)]
          (if (= target (symbol "js" "Object"))
            (let [global (native-object-global compiler phase)]
              (if global global
                (record-facts-error
                  "js/Object resolution requires its genuine typed compiler fact"
                  {:symbol protocol-symbol :phase phase})))
            (require-binding live current namespaces signatures target protocol-symbol)))
        (let [captured (get resolved-bindings protocol-symbol)]
          (if captured captured
        (let [local protocol-symbol
              referred (get refers local)]
          (if referred
            (require-binding live current namespaces signatures referred protocol-symbol)
            (let [declaration (and own-definitions (contains? own-definitions local))
                  signature (get own-signatures local)]
              (if (or declaration signature)
                (if signature signature (symbol own-namespace symbol-name))
                (if (contains? exclusions local)
                  (record-facts-error
                    "Record protocol symbol is unresolved in the caller namespace"
                    {:symbol protocol-symbol :phase phase :current current})
                  (let [declared-core (get core-signatures local)
                        core (if declared-core declared-core (symbol "suss.core" symbol-name))]
                    (if (or declared-core
                            (and core-definitions (contains? core-definitions local))
                            (contains? signatures core))
                      core
                      (record-facts-error
                        "Record protocol symbol is unresolved in the caller namespace"
                        {:symbol protocol-symbol :phase phase :current current})))))))))))))))))

(def ^:private resolve-protocol-symbol
  (fn [env protocol-symbol]
    ((make-protocol-resolver (compiler-state env)) protocol-symbol)))

(def ^:private build-record-mask-data
  (fn
    ([impls] (build-record-mask-data impls nil))
    ([impls seed]
    (let [facts
          (loop [remaining (seq impls)
                 protocols (if seed (:protocols seed) #{})
                 skip (if seed (:skip-protocol-flag seed) #{})
                 masks (if seed (:protocol-masks seed) {})
                 selected (if seed true false)
                 all-fast true]
            (if remaining
              (let [form (first remaining)]
                (if (symbol? form)
                  (let [global form
                        entry (fast-path-entry global)
                        next-protocols (conj protocols global)]
                    (recur (next remaining)
                      next-protocols
                      ;; Until a non-fast protocol appears these sets have
                      ;; exactly the same contents. Share the persistent result;
                      ;; thereafter retain the ordinary independent skip set.
                      (if entry
                        (if all-fast next-protocols (conj skip global))
                        skip)
                      (if entry
                        (let [partition (second entry) bit (nth entry 2)]
                          (assoc masks partition
                            (suss.core/bit-or (get masks partition 0) bit)))
                        masks)
                      (if entry true selected)
                      (if entry all-fast false)))
                  (recur (next remaining) protocols skip masks selected all-fast)))
              [protocols skip masks selected]))
          protocols (nth facts 0)
          skip (nth facts 1)
          masks (nth facts 2)
          selected (nth facts 3)]
      (if selected
        (let [masks
              (loop [partition 0 result masks]
                (if (< partition fast-path-partitions-count)
                  (recur (inc partition)
                    (if (contains? result partition)
                      result
                      (assoc result partition 0)))
                  result))]
          {:protocols protocols :skip-protocol-flag skip :protocol-masks masks})
        {:protocols protocols :skip-protocol-flag nil :protocol-masks nil})))))

;; Pure immutable data derived from every retained default protocol/table row.
;; It makes no declaration or binding claim. Actual caller resolution below is
;; required before any cache reuse, including aliases, exclusions and shadows.
(def ^:private default-record-globals '[suss.core/IRecord suss.core/ICloneable suss.core/IHash suss.core/IEquiv suss.core/IMeta suss.core/IWithMeta suss.core/ILookup suss.core/ICounted suss.core/ICollection suss.core/IAssociative suss.core/IMap suss.core/ISeqable suss.core/IIterable suss.core/IPrintWithWriter suss.core/IKVReduce])
(def ^:private default-record-protocol-set (quote #{suss.core/IRecord suss.core/ICloneable suss.core/IHash suss.core/IEquiv suss.core/IMeta suss.core/IWithMeta suss.core/ILookup suss.core/ICounted suss.core/ICollection suss.core/IAssociative suss.core/IMap suss.core/ISeqable suss.core/IIterable suss.core/IPrintWithWriter suss.core/IKVReduce}))
(def ^:private default-record-mask-data {:protocols default-record-protocol-set :skip-protocol-flag default-record-protocol-set :protocol-masks {0 -2064251126 1 139264}})
(def resolve-record-compiler-facts
  (fn [env impls]
    (let [resolve (make-protocol-resolver (compiler-state env))
          globals (loop [remaining (seq impls) result (transient [])]
                    (if remaining
                      (let [form (first remaining)]
                        (recur (next remaining)
                          (if (symbol? form) (conj! result (resolve form)) result)))
                      (persistent! result)))]
      ;; Mask construction is pure set union and bit-or. Reuse the complete
      ;; default result only after its actual resolved suffix matches; process
      ;; every user protocol through the generic path, including duplicates.
      (let [prefix-count (- (count globals) (count default-record-globals))]
        (if (and (<= 0 prefix-count)
                 (= (subvec globals prefix-count) default-record-globals))
          (if (zero? prefix-count)
            default-record-mask-data
            (build-record-mask-data (subvec globals 0 prefix-count)
              default-record-mask-data))
          (build-record-mask-data globals))))))

"####;
