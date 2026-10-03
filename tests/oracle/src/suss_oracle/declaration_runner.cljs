(ns suss-oracle.declaration-runner
  (:require-macros [suss-oracle.declaration-macros :refer [observe observe-locals]]))

(def initial (observe "initial" [pending scalar fixed multiple alias *dynamic*]))
(def pending)
(def ^{:private true :doc "symbol documentation"} scalar
  "explicit documentation"
  (observe "scalar-initializer" [pending scalar]))
(def after-scalar (observe "after-scalar" [pending scalar]))
(def fixed (fn [x] (observe "fixed-body" [fixed scalar]) x))
(def after-fixed (observe "after-fixed" [fixed scalar]))
(def multiple
  (fn self
    ([x] (observe "multiple-fixed-body" [multiple fixed]) x)
    ([x y & more] (observe "multiple-rest-body" [multiple fixed]) y)))
(def after-multiple (observe "after-multiple" [multiple fixed]))
(def alias fixed)
(def ^:dynamic *dynamic* 7)
(def after-alias (observe "after-alias" [alias *dynamic*]))
(def ^string hinted (observe "hinted-initializer" [hinted]))
(def after-hinted (observe "after-hinted" [hinted]))
(def scalar (observe "scalar-redefinition" [scalar]))
(def after-redefinition (observe "after-redefinition" [scalar]))

(def nested-wrapper
  (do (def nested-first 1)
      (observe "nested-after-first" [nested-first nested-second nested-wrapper])
      (def nested-second
        (observe "nested-second-initializer" [nested-first nested-second nested-wrapper]))
      (observe "nested-after-second" [nested-first nested-second nested-wrapper])))
(def after-nested (observe "after-nested" [nested-first nested-second nested-wrapper]))
(declare declared)
(def after-declare (observe "after-declare" [declared]))
(def declared (fn [x] x))
(def after-declared-definition (observe "after-declared-definition" [declared]))
(def duplicate (fn ([x] 1) ([y] 2)))
(def after-duplicate (observe "after-duplicate" [duplicate]))
(def named
  (fn self
    ([x] (observe-locals "self-fixed-body" [self x more]) x)
    ([x y & more] (observe-locals "self-rest-body" [self x y more]) y)))
(def after-named (observe "after-named" [named]))

(defn -main []
  (println (.stringify js/JSON
            #js [initial scalar after-scalar (fixed 11) after-fixed
                 (multiple 12) (multiple 12 13 14) after-multiple
                 (alias 15) *dynamic* after-alias hinted after-hinted
                 after-redefinition nested-wrapper nested-first nested-second
                 after-nested after-declare (declared 16) after-declared-definition
                 (duplicate 17) after-duplicate (named 18) (named 18 19 20)
                 after-named])))
(set! *main-cli-fn* -main)
