(ns suss-oracle.declaration-runner
  (:require-macros [suss-oracle.declaration-macros :refer [observe]]))

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

(defn -main []
  (println (.stringify js/JSON
            #js [initial scalar after-scalar (fixed 11) after-fixed
                 (multiple 12) (multiple 12 13 14) after-multiple
                 (alias 15) *dynamic* after-alias hinted after-hinted
                 after-redefinition])))
(set! *main-cli-fn* -main)
