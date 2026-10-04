(ns suss-oracle.global-reference-identity-probe
  (:require [suss-oracle.declaration-macros :as declaration]))

;; Original development-only pinned cross-invocation identity observation.
;; Keep a genuine declaration metadata value across separate macro invocations.
(defmacro observe [label binding]
  (let [init (get-in &env [:locals binding :init])
        info (:info init)
        metadata (:meta info)]
    (defonce saved metadata)
    (defonce saved-info info)
    (spit "out/global-reference-info-identity.jsonl"
      (str (declaration/json (identical? saved-info info)) "\n") :append true)
    (let [row [label (identical? saved metadata) (:doc metadata)]]
      (spit "out/global-reference-cross-invocation.jsonl"
            (str (declaration/json row) "\n") :append true)
      (list 'quote row))))
