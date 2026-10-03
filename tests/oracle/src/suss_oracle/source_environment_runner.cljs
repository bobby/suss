(ns suss-oracle.source-environment-runner
  (:require-macros [suss-oracle.source-environment-macros :refer [lexical-facts scope-facts]]))

(def effects (atom 0))
(def lexical
  (let [x (do (swap! effects inc) 7)]
    (let [x 9] (lexical-facts x))))
(def function (fn [x] (scope-facts)))
(def function-result (function 17))
(def variadic (let [x 7] (scope-facts x a b)))

(defn -main []
  (println (.stringify js/JSON
                      #js [(count lexical) @effects (count function-result)
                           (count variadic)])))
(set! *main-cli-fn* -main)
