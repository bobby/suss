(ns graph.namespace-runner (:require graph.app graph.nomac))
(defn -main [] (println (.stringify js/JSON #js [graph.app/observed graph.blank/observed graph.nomac/observed])))
(set! *main-cli-fn* -main)
