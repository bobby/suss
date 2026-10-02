(ns suss-oracle.environment-origin
  (:require-macros [suss-oracle.environment-macros :refer [position]]))
(def ascii (position))
(def text "𝄞") (def unicode (position))
(def crlf (position))(def cronly (position))
(defn -main [] (println (.stringify js/JSON (clj->js [ascii unicode crlf cronly]))))
(set! *main-cli-fn* -main)
