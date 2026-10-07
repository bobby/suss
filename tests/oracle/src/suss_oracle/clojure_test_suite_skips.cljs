(ns suss-oracle.clojure-test-suite-skips
  "Captures the suite's `when-var-exists` SKIP lines with their namespace or test.

  Load-time skips are attributed through generated marker namespaces that the
  loader requires immediately before each suite namespace.")

(def ^:private loading (atom nil))
(def ^:private current-test (atom (constantly nil)))
(def ^:private skips (atom []))

(defn- console-print [& args] (.apply (.-log js/console) js/console (into-array args)))

(defn- capturing [delegate]
  (fn [& args]
    (let [text (apply str args)]
      (if-let [[_ symbol] (re-matches #"SKIP - (\S+)\s*" text)]
        (let [test (@current-test)]
          (swap! skips conj #js {:symbol symbol
                                 :phase (if test "run" "load")
                                 :namespace (or (some-> test (.split "/") first) @loading)
                                 :test test}))
        (apply delegate args)))))

(defn loading! [namespace]
  (reset! loading namespace))

(defn capture! [test-fn]
  (reset! loading nil)
  (reset! current-test test-fn)
  (set! *print-fn* (capturing (or *print-fn* console-print)))
  (set! *print-newline* false))

(defn observed [] @skips)

(set! *print-fn* (capturing console-print))
(set! *print-newline* false)
