(defmacro inspect-nested-immutable-meta [f maker]
  (let [m (meta f) a (:item m) b (:again m) returned (maker)]
    (list 'quote [(identical? a b) (identical? a returned)
                  (identical? (meta a) (meta returned)) (= a returned)])))

(prn [(let [v (with-meta [42] {:tag :kept}) maker (fn [] v) f (with-meta (fn [] 1) {:item v :again v})] (eval (list 'inspect-nested-immutable-meta f maker)))
(let [v (with-meta [] {:tag :kept}) maker (fn [] v) f (with-meta (fn [] 1) {:item v :again v})] (eval (list 'inspect-nested-immutable-meta f maker)))
(let [v (with-meta '(42) {:tag :kept}) maker (fn [] v) f (with-meta (fn [] 1) {:item v :again v})] (eval (list 'inspect-nested-immutable-meta f maker)))
(let [v (with-meta '() {:tag :kept}) maker (fn [] v) f (with-meta (fn [] 1) {:item v :again v})] (eval (list 'inspect-nested-immutable-meta f maker)))
(let [v (with-meta {:answer 42} {:tag :kept}) maker (fn [] v) f (with-meta (fn [] 1) {:item v :again v})] (eval (list 'inspect-nested-immutable-meta f maker)))
(let [v (with-meta {:a 1 :b 2 :c 3 :d 4 :e 5 :f 6 :g 7 :h 8 :i 9} {:tag :kept}) maker (fn [] v) f (with-meta (fn [] 1) {:item v :again v})] (eval (list 'inspect-nested-immutable-meta f maker)))
(let [v (with-meta #{42} {:tag :kept}) maker (fn [] v) f (with-meta (fn [] 1) {:item v :again v})] (eval (list 'inspect-nested-immutable-meta f maker)))
(let [v (with-meta 'saved {:tag :kept}) maker (fn [] v) f (with-meta (fn [] 1) {:item v :again v})] (eval (list 'inspect-nested-immutable-meta f maker)))
(let [v (let [m {:tag :kept} v (with-meta [42] m)] (with-meta {:a v :b v} m)) maker (fn [] v) f (with-meta (fn [] 1) {:item v :again v})] (eval (list 'inspect-nested-immutable-meta f maker)))])
