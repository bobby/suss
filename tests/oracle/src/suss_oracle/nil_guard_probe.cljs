(ns suss-oracle.nil-guard-probe)
(defn my-str [x] (if (nil? x) "" (.toString x)))
(defn probe []
  [(str "a")
   (let [old nil?] (try (do (set! nil? (fn [_] true)) [(str "b") (my-str "c") (nil? 5)]) (finally (set! nil? old))))])
(prn (probe))
