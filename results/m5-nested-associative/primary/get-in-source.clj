(require '[clojure.repl :refer [source]])
(source get-in)
(let [called (atom [])]
 (with-redefs [get (fn [& args] (swap! called conj (vec args)) :shadow)]
  (prn [(get-in {:a {:b 17}} [:a :b]) (get-in {:a {:b 17}} [:a :b] :missing) @called])))
