(ns suss-oracle.metadata-position
  (:require-macros [suss-oracle.environment-macros :refer [inspect-local]]))
(def plain (let [x 1] (inspect-local x)))
(def tagged (let [^number x 1] (inspect-local x)))
(def chained (let [^:private ^number x 1] (inspect-local x)))
(def mapped (let [^{:tag number :private true} x 1] (inspect-local x)))
(defn -main [] (println (.stringify js/JSON (clj->js [plain tagged chained mapped]))))
(set! *main-cli-fn* -main)
