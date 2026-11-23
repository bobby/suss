(assert (= "1.12.6" (clojure-version)))
(println (pr-str (pr-str (let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 value (with-redefs [= (fn [left right] (swap! order conj [left right]) (original left right))]
   (case (do (swap! selector-count inc) false)
    (false nil 0 1 2 :hit "seven" true :late) (do (swap! arms #(+ (* % 10) 1)) 17)
    :other (do (swap! arms #(+ (* % 10) 2)) 19)
    (3 "tail") (do (swap! arms #(+ (* % 10) 3)) 21)
    (do (swap! arms #(+ (* % 10) 4)) 23)))]
 [value @selector-count @arms @order]))))
(println (pr-str (pr-str (let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 value (with-redefs [= (fn [left right] (swap! order conj [left right]) (original left right))]
   (case (do (swap! selector-count inc) nil)
    (false nil 0 1 2 :hit "seven" true :late) (do (swap! arms #(+ (* % 10) 1)) 17)
    :other (do (swap! arms #(+ (* % 10) 2)) 19)
    (3 "tail") (do (swap! arms #(+ (* % 10) 3)) 21)
    (do (swap! arms #(+ (* % 10) 4)) 23)))]
 [value @selector-count @arms @order]))))
(println (pr-str (pr-str (let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 value (with-redefs [= (fn [left right] (swap! order conj [left right]) (original left right))]
   (case (do (swap! selector-count inc) 0)
    (false nil 0 1 2 :hit "seven" true :late) (do (swap! arms #(+ (* % 10) 1)) 17)
    :other (do (swap! arms #(+ (* % 10) 2)) 19)
    (3 "tail") (do (swap! arms #(+ (* % 10) 3)) 21)
    (do (swap! arms #(+ (* % 10) 4)) 23)))]
 [value @selector-count @arms @order]))))
(println (pr-str (pr-str (let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 value (with-redefs [= (fn [left right] (swap! order conj [left right]) (original left right))]
   (case (do (swap! selector-count inc) 2)
    (false nil 0 1 2 :hit "seven" true :late) (do (swap! arms #(+ (* % 10) 1)) 17)
    :other (do (swap! arms #(+ (* % 10) 2)) 19)
    (3 "tail") (do (swap! arms #(+ (* % 10) 3)) 21)
    (do (swap! arms #(+ (* % 10) 4)) 23)))]
 [value @selector-count @arms @order]))))
(println (pr-str (pr-str (let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 value (with-redefs [= (fn [left right] (swap! order conj [left right]) (original left right))]
   (case (do (swap! selector-count inc) "seven")
    (false nil 0 1 2 :hit "seven" true :late) (do (swap! arms #(+ (* % 10) 1)) 17)
    :other (do (swap! arms #(+ (* % 10) 2)) 19)
    (3 "tail") (do (swap! arms #(+ (* % 10) 3)) 21)
    (do (swap! arms #(+ (* % 10) 4)) 23)))]
 [value @selector-count @arms @order]))))
(println (pr-str (pr-str (let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 value (with-redefs [= (fn [left right] (swap! order conj [left right]) (original left right))]
   (case (do (swap! selector-count inc) true)
    (false nil 0 1 2 :hit "seven" true :late) (do (swap! arms #(+ (* % 10) 1)) 17)
    :other (do (swap! arms #(+ (* % 10) 2)) 19)
    (3 "tail") (do (swap! arms #(+ (* % 10) 3)) 21)
    (do (swap! arms #(+ (* % 10) 4)) 23)))]
 [value @selector-count @arms @order]))))
(println (pr-str (pr-str (let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 value (with-redefs [= (fn [left right] (swap! order conj [left right]) (original left right))]
   (case (do (swap! selector-count inc) :late)
    (false nil 0 1 2 :hit "seven" true :late) (do (swap! arms #(+ (* % 10) 1)) 17)
    :other (do (swap! arms #(+ (* % 10) 2)) 19)
    (3 "tail") (do (swap! arms #(+ (* % 10) 3)) 21)
    (do (swap! arms #(+ (* % 10) 4)) 23)))]
 [value @selector-count @arms @order]))))
(println (pr-str (pr-str (let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 value (with-redefs [= (fn [left right] (swap! order conj [left right]) (original left right))]
   (case (do (swap! selector-count inc) :other)
    (false nil 0 1 2 :hit "seven" true :late) (do (swap! arms #(+ (* % 10) 1)) 17)
    :other (do (swap! arms #(+ (* % 10) 2)) 19)
    (3 "tail") (do (swap! arms #(+ (* % 10) 3)) 21)
    (do (swap! arms #(+ (* % 10) 4)) 23)))]
 [value @selector-count @arms @order]))))
(println (pr-str (pr-str (let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 value (with-redefs [= (fn [left right] (swap! order conj [left right]) (original left right))]
   (case (do (swap! selector-count inc) "tail")
    (false nil 0 1 2 :hit "seven" true :late) (do (swap! arms #(+ (* % 10) 1)) 17)
    :other (do (swap! arms #(+ (* % 10) 2)) 19)
    (3 "tail") (do (swap! arms #(+ (* % 10) 3)) 21)
    (do (swap! arms #(+ (* % 10) 4)) 23)))]
 [value @selector-count @arms @order]))))
(println (pr-str (pr-str (let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 value (with-redefs [= (fn [left right] (swap! order conj [left right]) (original left right))]
   (case (do (swap! selector-count inc) :missing)
    (false nil 0 1 2 :hit "seven" true :late) (do (swap! arms #(+ (* % 10) 1)) 17)
    :other (do (swap! arms #(+ (* % 10) 2)) 19)
    (3 "tail") (do (swap! arms #(+ (* % 10) 3)) 21)
    (do (swap! arms #(+ (* % 10) 4)) 23)))]
 [value @selector-count @arms @order]))))
(println (pr-str (pr-str (let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 result (try (with-redefs [= (fn [left right] (swap! order conj left)
  (if (original left 0) (throw (ex-info "late equality" {:value 91})) (original left right)))]
  (case (do (swap! selector-count inc) 0)
   (false nil 0 1 2 :hit "seven" true :late) (do (reset! arms 1) 17)
   :other (do (reset! arms 2) 19) (do (reset! arms 3) 23)))
  (catch clojure.lang.ExceptionInfo e [:thrown (ex-message e) (ex-data e)]))]
 [result @selector-count @arms @order (identical? original =)]))))
(println (pr-str (pr-str (try (macroexpand '(case :late (false nil 0 1 2 :hit "seven" true :late) 17 :late 19 23)) (catch Throwable e (loop [e e] (if (ex-cause e) (recur (ex-cause e)) (ex-message e))))))))
(println (pr-str (pr-str (try (macroexpand '(case :missing (false nil 0 1 2 :hit "seven" true :late) 17 nil 19 23)) (catch Throwable e (loop [e e] (if (ex-cause e) (recur (ex-cause e)) (ex-message e))))))))
(println (pr-str (pr-str (try (macroexpand '(case 0 (false nil 0 1 2 :hit "seven" true 0) 17 23)) (catch Throwable e (loop [e e] (if (ex-cause e) (recur (ex-cause e)) (ex-message e))))))))
