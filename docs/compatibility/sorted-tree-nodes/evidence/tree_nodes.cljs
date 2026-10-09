(ns suss-oracle.tree-nodes)
(def BlackNode cljs.core/BlackNode)
(def RedNode cljs.core/RedNode)
(def balance-left cljs.core/balance-left)
(def balance-right cljs.core/balance-right)
(def balance-left-del cljs.core/balance-left-del)
(def balance-right-del cljs.core/balance-right-del)
(def tree-map-kv-reduce cljs.core/tree-map-kv-reduce)
(def tree-node-shape
  (fn tree-node-shape [node]
    (if (nil? node)
      nil
      [(if (instance? BlackNode node) :black :red)
       (.-key node) (.-val node)
       (tree-node-shape (.-left node))
       (tree-node-shape (.-right node))])))

(defn -main [] (println (.stringify js/JSON #js [
#js {:id "balance-left-nonred" :value (= (tree-node-shape (balance-left 6 60 (BlackNode. 3 30 nil nil nil) nil)) [:black 6 60 [:black 3 30 nil nil] nil])}
#js {:id "balance-left-red-no-red-child" :value (= (tree-node-shape (balance-left 6 60 (RedNode. 3 30 nil nil nil) nil)) [:black 6 60 [:red 3 30 nil nil] nil])}
#js {:id "balance-left-outer-red" :value (= (tree-node-shape (balance-left 6 60 (RedNode. 3 30 (RedNode. 1 10 nil nil nil) nil nil) nil)) [:red 3 30 [:black 1 10 nil nil] [:black 6 60 nil nil]])}
#js {:id "balance-left-inner-red" :value (= (tree-node-shape (balance-left 6 60 (RedNode. 3 30 nil (RedNode. 4 40 nil nil nil) nil) nil)) [:red 4 40 [:black 3 30 nil nil] [:black 6 60 nil nil]])}
#js {:id "balance-left-outer-priority" :value (= (tree-node-shape (balance-left 6 60 (RedNode. 3 30 (RedNode. 1 10 nil nil nil) (RedNode. 4 40 nil nil nil) nil) nil)) [:red 3 30 [:black 1 10 nil nil] [:black 6 60 [:red 4 40 nil nil] nil]])}
#js {:id "balance-right-nonred" :value (= (tree-node-shape (balance-right 2 20 nil (BlackNode. 5 50 nil nil nil))) [:black 2 20 nil [:black 5 50 nil nil]])}
#js {:id "balance-right-red-no-red-child" :value (= (tree-node-shape (balance-right 2 20 nil (RedNode. 5 50 nil nil nil))) [:black 2 20 nil [:red 5 50 nil nil]])}
#js {:id "balance-right-outer-red" :value (= (tree-node-shape (balance-right 2 20 nil (RedNode. 5 50 nil (RedNode. 7 70 nil nil nil) nil))) [:red 5 50 [:black 2 20 nil nil] [:black 7 70 nil nil]])}
#js {:id "balance-right-inner-red" :value (= (tree-node-shape (balance-right 2 20 nil (RedNode. 5 50 (RedNode. 3 30 nil nil nil) nil nil))) [:red 3 30 [:black 2 20 nil nil] [:black 5 50 nil nil]])}
#js {:id "balance-right-outer-priority" :value (= (tree-node-shape (balance-right 2 20 nil (RedNode. 5 50 (RedNode. 3 30 nil nil nil) (RedNode. 7 70 nil nil nil) nil))) [:red 5 50 [:black 2 20 nil [:red 3 30 nil nil]] [:black 7 70 nil nil]])}
#js {:id "left-del-red" :value (= (tree-node-shape (balance-left-del 6 60 (RedNode. 3 30 nil nil nil) (BlackNode. 9 90 nil nil nil))) [:red 6 60 [:black 3 30 nil nil] [:black 9 90 nil nil]])}
#js {:id "right-del-red" :value (= (tree-node-shape (balance-right-del 6 60 (BlackNode. 3 30 nil nil nil) (RedNode. 9 90 nil nil nil))) [:red 6 60 [:black 3 30 nil nil] [:black 9 90 nil nil]])}
#js {:id "left-del-black-sibling" :value (= (tree-node-shape (balance-left-del 6 60 nil (BlackNode. 9 90 nil nil nil))) [:black 6 60 nil [:red 9 90 nil nil]])}
#js {:id "right-del-black-sibling" :value (= (tree-node-shape (balance-right-del 6 60 (BlackNode. 3 30 nil nil nil) nil)) [:black 6 60 [:red 3 30 nil nil] nil])}
#js {:id "left-del-red-black-inner" :value (= (tree-node-shape (balance-left-del 6 60 nil (RedNode. 9 90 (BlackNode. 7 70 nil nil nil) (BlackNode. 11 110 nil nil nil) nil))) [:red 7 70 [:black 6 60 nil nil] [:black 9 90 nil [:red 11 110 nil nil]]])}
#js {:id "right-del-red-black-inner" :value (= (tree-node-shape (balance-right-del 6 60 (RedNode. 3 30 (BlackNode. 1 10 nil nil nil) (BlackNode. 5 50 nil nil nil) nil) nil)) [:red 5 50 [:black 3 30 [:red 1 10 nil nil] nil] [:black 6 60 nil nil]])}
#js {:id "balance-left-del-invariant" :value (try (balance-left-del 6 60 nil nil) false (catch :default e (= (.-message e) "red-black tree invariant violation")))}
#js {:id "balance-right-del-invariant" :value (try (balance-right-del 6 60 nil nil) false (catch :default e (= (.-message e) "red-black tree invariant violation")))}
#js {:id "blacknode-key-val-count" :value (let [n (BlackNode. 2 20 nil nil nil)] (and (= (key n) 2) (= (val n) 20) (= (count n) 2)))}
#js {:id "blacknode-nth-get-invoke" :value (let [n (BlackNode. 2 20 nil nil nil)] (and (= (nth n 0) 2) (= (nth n 1) 20) (= (nth n 2 17) 17) (nil? (get n 2)) (= (get n 2 17) 17) (= (n 0) 2) (= (n 9 17) 17)))}
#js {:id "blacknode-nth-error" :value (let [n (BlackNode. 2 20 nil nil nil)] (try (nth n 2) false (catch :default e (= (.-message e) "Index out of bounds"))))}
#js {:id "blacknode-invoke-error" :value (let [n (BlackNode. 2 20 nil nil nil)] (try (n 2) false (catch :default e (= (.-message e) "Index out of bounds"))))}
#js {:id "blacknode-meta-with-meta" :value (let [n (BlackNode. 2 20 nil nil nil)] (and (nil? (meta n)) (= (meta (with-meta n {:tag 17})) {:tag 17}) (= (with-meta n {:tag 17}) [2 20]) (vector? (with-meta n {:tag 17}))))}
#js {:id "blacknode-stack-conj-empty" :value (let [n (BlackNode. 2 20 nil nil nil)] (and (= (peek n) 20) (= (pop n) [2]) (= (conj n 17) [2 20 17]) (nil? (empty n))))}
#js {:id "blacknode-seq-rseq" :value (let [n (BlackNode. 2 20 nil nil nil)] (and (= (seq n) (list 2 20)) (= (rseq n) (list 20 2))))}
#js {:id "blacknode-assoc-vector" :value (let [n (BlackNode. 2 20 nil nil nil)] (and (= (assoc n 0 17) [17 20]) (= (assoc n 1 17) [2 17]) (= (assoc n 2 17) [2 20 17]) (= (-assoc-n n 1 17) [2 17])))}
#js {:id "blacknode-contains-find" :value (let [n (BlackNode. 2 20 nil nil nil)] (and (contains? n 0) (contains? n 1) (not (contains? n 2)) (= (find n 0) [0 2]) (= (find n 1) [1 20]) (nil? (find n 2))))}
#js {:id "blacknode-equiv" :value (let [n (BlackNode. 2 20 nil nil nil)] (and (= n [2 20]) (= n (list 2 20)) (not (= n [2 21])) (not (= n [2])) (vector? n)))}
#js {:id "blacknode-reduce" :value (let [n (BlackNode. 2 20 nil nil nil)] (and (= (reduce + n) 22) (= (reduce + 10 n) 32) (= (reduce (fn [_ _] (reduced 17)) n) 17)))}
#js {:id "blacknode-indexOf" :value (let [n (BlackNode. 2 20 nil nil nil)] (and (= (.indexOf n 2) 0) (= (.indexOf n 20) 1) (= (.indexOf n 2 1) -1) (= (.lastIndexOf n 20) 1) (= (.lastIndexOf n 2 0) 0)))}
#js {:id "blacknode-replace-color" :value (let [n (BlackNode. 2 20 nil nil nil)] (let [m (.replace n 3 30 nil nil)] (and (instance? BlackNode m) (= (key m) 3) (= (val m) 30) (nil? (.-__hash m)))))}
#js {:id "blacknode-hash-cache-zero" :value (let [n (BlackNode. 2 20 nil nil nil)] (let [h hash-ordered-coll calls (atom 0) m (BlackNode. 2 20 nil nil 0)] (try (set! hash-ordered-coll (fn [_] (swap! calls inc) 17)) (and (= (hash m) 0) (= @calls 0)) (finally (set! hash-ordered-coll h)))))}
#js {:id "blacknode-hash-cache-success" :value (let [n (BlackNode. 2 20 nil nil nil)] (let [h hash-ordered-coll calls (atom 0)] (try (set! hash-ordered-coll (fn [_] (swap! calls inc) 17)) (and (= (hash n) 17) (= (hash n) 17) (= @calls 1) (= (.-__hash n) 17)) (finally (set! hash-ordered-coll h)))))}
#js {:id "blacknode-hash-cache-throw" :value (let [n (BlackNode. 2 20 nil nil nil)] (let [h hash-ordered-coll] (try (set! hash-ordered-coll (fn [_] (throw 73))) (try (hash n) false (catch :default e (and (= e 73) (nil? (.-__hash n))))) (finally (set! hash-ordered-coll h)))))}
#js {:id "rednode-key-val-count" :value (let [n (RedNode. 2 20 nil nil nil)] (and (= (key n) 2) (= (val n) 20) (= (count n) 2)))}
#js {:id "rednode-nth-get-invoke" :value (let [n (RedNode. 2 20 nil nil nil)] (and (= (nth n 0) 2) (= (nth n 1) 20) (= (nth n 2 17) 17) (nil? (get n 2)) (= (get n 2 17) 17) (= (n 0) 2) (= (n 9 17) 17)))}
#js {:id "rednode-nth-error" :value (let [n (RedNode. 2 20 nil nil nil)] (try (nth n 2) false (catch :default e (= (.-message e) "Index out of bounds"))))}
#js {:id "rednode-invoke-error" :value (let [n (RedNode. 2 20 nil nil nil)] (try (n 2) false (catch :default e (= (.-message e) "Index out of bounds"))))}
#js {:id "rednode-meta-with-meta" :value (let [n (RedNode. 2 20 nil nil nil)] (and (nil? (meta n)) (= (meta (with-meta n {:tag 17})) {:tag 17}) (= (with-meta n {:tag 17}) [2 20]) (vector? (with-meta n {:tag 17}))))}
#js {:id "rednode-stack-conj-empty" :value (let [n (RedNode. 2 20 nil nil nil)] (and (= (peek n) 20) (= (pop n) [2]) (= (conj n 17) [2 20 17]) (nil? (empty n))))}
#js {:id "rednode-seq-rseq" :value (let [n (RedNode. 2 20 nil nil nil)] (and (= (seq n) (list 2 20)) (= (rseq n) (list 20 2))))}
#js {:id "rednode-assoc-vector" :value (let [n (RedNode. 2 20 nil nil nil)] (and (= (assoc n 0 17) [17 20]) (= (assoc n 1 17) [2 17]) (= (assoc n 2 17) [2 20 17]) (= (-assoc-n n 1 17) [2 17])))}
#js {:id "rednode-contains-find" :value (let [n (RedNode. 2 20 nil nil nil)] (and (contains? n 0) (contains? n 1) (not (contains? n 2)) (= (find n 0) [0 2]) (= (find n 1) [1 20]) (nil? (find n 2))))}
#js {:id "rednode-equiv" :value (let [n (RedNode. 2 20 nil nil nil)] (and (= n [2 20]) (= n (list 2 20)) (not (= n [2 21])) (not (= n [2])) (vector? n)))}
#js {:id "rednode-reduce" :value (let [n (RedNode. 2 20 nil nil nil)] (and (= (reduce + n) 22) (= (reduce + 10 n) 32) (= (reduce (fn [_ _] (reduced 17)) n) 17)))}
#js {:id "rednode-indexOf" :value (let [n (RedNode. 2 20 nil nil nil)] (and (= (.indexOf n 2) 0) (= (.indexOf n 20) 1) (= (.indexOf n 2 1) -1) (= (.lastIndexOf n 20) 1) (= (.lastIndexOf n 2 0) 0)))}
#js {:id "rednode-replace-color" :value (let [n (RedNode. 2 20 nil nil nil)] (let [m (.replace n 3 30 nil nil)] (and (instance? RedNode m) (= (key m) 3) (= (val m) 30) (nil? (.-__hash m)))))}
#js {:id "rednode-hash-cache-zero" :value (let [n (RedNode. 2 20 nil nil nil)] (let [h hash-ordered-coll calls (atom 0) m (RedNode. 2 20 nil nil 0)] (try (set! hash-ordered-coll (fn [_] (swap! calls inc) 17)) (and (= (hash m) 0) (= @calls 0)) (finally (set! hash-ordered-coll h)))))}
#js {:id "rednode-hash-cache-success" :value (let [n (RedNode. 2 20 nil nil nil)] (let [h hash-ordered-coll calls (atom 0)] (try (set! hash-ordered-coll (fn [_] (swap! calls inc) 17)) (and (= (hash n) 17) (= (hash n) 17) (= @calls 1) (= (.-__hash n) 17)) (finally (set! hash-ordered-coll h)))))}
#js {:id "rednode-hash-cache-throw" :value (let [n (RedNode. 2 20 nil nil nil)] (let [h hash-ordered-coll] (try (set! hash-ordered-coll (fn [_] (throw 73))) (try (hash n) false (catch :default e (and (= e 73) (nil? (.-__hash n))))) (finally (set! hash-ordered-coll h)))))}
#js {:id "black-blacken-owner-identity" :value (let [n (BlackNode. 2 20 nil nil nil)] (identical? (.blacken n) n))}
#js {:id "black-redden-sharing" :value (let [l (BlackNode. 1 10 nil nil nil) r (BlackNode. 3 30 nil nil nil) n (BlackNode. 2 20 l r 17) m (.redden n)] (and (instance? RedNode m) (identical? (.-left m) l) (identical? (.-right m) r) (nil? (.-__hash m)) (= (.-__hash n) 17)))}
#js {:id "red-blacken-sharing" :value (let [l (BlackNode. 1 10 nil nil nil) r (BlackNode. 3 30 nil nil nil) n (RedNode. 2 20 l r 17) m (.blacken n)] (and (instance? BlackNode m) (identical? (.-left m) l) (identical? (.-right m) r) (nil? (.-__hash m)) (= (.-__hash n) 17)))}
#js {:id "red-redden-invariant" :value (try (.redden (RedNode. 2 20 nil nil nil)) false (catch :default e (= (.-message e) "red-black tree invariant violation")))}
#js {:id "red-add-remove-children" :value (let [n (RedNode. 2 20 nil nil nil) l (BlackNode. 1 10 nil nil nil) r (BlackNode. 3 30 nil nil nil)] (and (identical? (.-left (.add-left n l)) l) (identical? (.-right (.add-right n r)) r) (identical? (.-left (.remove-left n l)) l) (identical? (.-right (.remove-right n r)) r)))}
#js {:id "black-method-insertion-dispatch" :value (let [n (BlackNode. 2 20 nil nil nil)] (and (= (tree-node-shape (.add-left n (RedNode. 1 10 nil nil nil))) [:black 2 20 [:red 1 10 nil nil] nil]) (= (tree-node-shape (.add-right n (RedNode. 3 30 nil nil nil))) [:black 2 20 nil [:red 3 30 nil nil]])))}
#js {:id "red-method-balance-left-outer" :value (= (tree-node-shape (.balance-left (RedNode. 3 30 (RedNode. 1 10 nil nil nil) nil nil) (BlackNode. 6 60 nil nil nil))) (tree-node-shape (balance-left 6 60 (RedNode. 3 30 (RedNode. 1 10 nil nil nil) nil nil) nil)))}
#js {:id "red-method-balance-left-inner" :value (= (tree-node-shape (.balance-left (RedNode. 3 30 nil (RedNode. 4 40 nil nil nil) nil) (BlackNode. 6 60 nil nil nil))) (tree-node-shape (balance-left 6 60 (RedNode. 3 30 nil (RedNode. 4 40 nil nil nil) nil) nil)))}
#js {:id "red-method-balance-left-none" :value (= (tree-node-shape (.balance-left (RedNode. 3 30 nil nil nil) (BlackNode. 6 60 nil nil nil))) (tree-node-shape (balance-left 6 60 (RedNode. 3 30 nil nil nil) nil)))}
#js {:id "red-method-balance-right-outer" :value (= (tree-node-shape (.balance-right (RedNode. 5 50 nil (RedNode. 7 70 nil nil nil) nil) (BlackNode. 2 20 nil nil nil))) (tree-node-shape (balance-right 2 20 nil (RedNode. 5 50 nil (RedNode. 7 70 nil nil nil) nil))))}
#js {:id "red-method-balance-right-inner" :value (= (tree-node-shape (.balance-right (RedNode. 5 50 (RedNode. 3 30 nil nil nil) nil nil) (BlackNode. 2 20 nil nil nil))) (tree-node-shape (balance-right 2 20 nil (RedNode. 5 50 (RedNode. 3 30 nil nil nil) nil nil))))}
#js {:id "red-method-balance-right-none" :value (= (tree-node-shape (.balance-right (RedNode. 5 50 nil nil nil) (BlackNode. 2 20 nil nil nil))) (tree-node-shape (balance-right 2 20 nil (RedNode. 5 50 nil nil nil))))}
#js {:id "kv-inorder-effects" :value (let [trace (atom []) result (tree-map-kv-reduce (BlackNode. 2 20 (BlackNode. 1 10 nil nil nil) (BlackNode. 3 30 nil nil nil) nil) (fn [a k v] (swap! trace conj k) (+ a v)) 0)] (and (= result 60) (= @trace [1 2 3])))}
#js {:id "kv-left-reduced-preserves-wrapper" :value (let [trace (atom []) stop (reduced false) result (tree-map-kv-reduce (BlackNode. 2 20 (BlackNode. 1 10 nil nil nil) (BlackNode. 3 30 nil nil nil) nil) (fn [_ k v] (swap! trace conj k) stop) 0)] (and (identical? result stop) (false? @result) (= @trace [1])))}
#js {:id "kv-root-reduced-preserves-wrapper" :value (let [trace (atom []) stop (reduced nil) result (tree-map-kv-reduce (BlackNode. 2 20 (BlackNode. 1 10 nil nil nil) (BlackNode. 3 30 nil nil nil) nil) (fn [a k v] (swap! trace conj k) (if (= k 2) stop (+ a v))) 0)] (and (identical? result stop) (nil? @result) (= @trace [1 2])))}
#js {:id "kv-initial-reduced-leftless" :value (let [stop (reduced 17)] (identical? (tree-map-kv-reduce (BlackNode. 2 20 nil nil nil) (fn [_ _ _] (throw 73)) stop) stop))}
#js {:id "kv-original-throw" :value (let [trace (atom [])] (try (tree-map-kv-reduce (BlackNode. 2 20 (BlackNode. 1 10 nil nil nil) (BlackNode. 3 30 nil nil nil) nil) (fn [_ k v] (swap! trace conj k) (throw 73)) 0) false (catch :default e (and (= e 73) (= @trace [1])))))}
#js {:id "blacknode-kv-method" :value (= (.kv-reduce (BlackNode. 2 20 (BlackNode. 1 10 nil nil nil) (BlackNode. 3 30 nil nil nil) nil) (fn [a k v] (+ a v)) 0) 60)}
#js {:id "rednode-kv-method" :value (= (.kv-reduce (RedNode. 2 20 (BlackNode. 1 10 nil nil nil) (BlackNode. 3 30 nil nil nil) nil) (fn [a k v] (+ a v)) 0) 60)}
#js {:id "blacknode-typed-nth-ignores-public-rebind" :value (let [n (BlackNode. 7 9 nil nil nil) saved -nth] (set! -nth (fn [& xs] (throw 73))) (try (and (= (get n 0) 7) (= (get n 1 :missing) 9) (= (n 0) 7) (= (n 1 :missing) 9)) (finally (set! -nth saved))))}
#js {:id "blacknode-typed-assoc-n-ignores-public-rebind" :value (let [n (BlackNode. 7 9 nil nil nil) saved -assoc-n] (set! -assoc-n (fn [& xs] (throw 73))) (try (= (saved n 0 3) [3 9]) (finally (set! -assoc-n saved))))}
#js {:id "rednode-typed-nth-ignores-public-rebind" :value (let [n (RedNode. 7 9 nil nil nil) saved -nth] (set! -nth (fn [& xs] (throw 73))) (try (and (= (get n 0) 7) (= (get n 1 :missing) 9) (= (n 0) 7) (= (n 1 :missing) 9)) (finally (set! -nth saved))))}
#js {:id "rednode-typed-assoc-n-ignores-public-rebind" :value (let [n (RedNode. 7 9 nil nil nil) saved -assoc-n] (set! -assoc-n (fn [& xs] (throw 73))) (try (= (saved n 0 3) [3 9]) (finally (set! -assoc-n saved))))}
#js {:id "comparable-symbol" :value (= (compare (symbol "a") (symbol "b")) -1)}
#js {:id "comparable-keyword" :value (= (compare :a :b) -1)}
#js {:id "comparable-subvec" :value (= (compare (subvec [0 7 9] 1) [7 10]) -1)}
#js {:id "comparable-vector" :value (= (compare [7 9] [7 10]) -1)}
#js {:id "comparable-mapentry" :value (= (compare (MapEntry. 7 9 nil) [7 10]) -1)}
#js {:id "blacknode-comparable-vector" :value (= (compare (BlackNode. 7 9 nil nil nil) [7 10]) -1)}
#js {:id "blacknode-comparable-other-node" :value (= (compare (BlackNode. 7 9 nil nil nil) (RedNode. 7 10 nil nil nil)) -1)}
#js {:id "blacknode-comparable-live-delegate" :value (let [saved cljs.core/compare-indexed calls (atom 0)] (set! cljs.core/compare-indexed (fn ([x y] (swap! calls inc) 17) ([x y len n] (throw 73)))) (try (and (= (compare (BlackNode. 7 9 nil nil nil) [7 10]) 17) (= @calls 1)) (finally (set! cljs.core/compare-indexed saved))))}
#js {:id "blacknode-comparable-rejection-format-order" :value (let [n (BlackNode. 7 9 nil nil nil) saved str_ trace (atom [])] (set! str_ (fn [x] (swap! trace conj x) (js-obj "toString" (fn [] (swap! trace conj 17) "z")))) (try (try (compare n "x") false (catch :default e (and (= (.-message e) "Cannot compare z to z") (= @trace [n "x" 17 17])))) (finally (set! str_ saved))))}
#js {:id "rednode-comparable-vector" :value (= (compare (RedNode. 7 9 nil nil nil) [7 10]) -1)}
#js {:id "rednode-comparable-other-node" :value (= (compare (RedNode. 7 9 nil nil nil) (RedNode. 7 10 nil nil nil)) -1)}
#js {:id "rednode-comparable-live-delegate" :value (let [saved cljs.core/compare-indexed calls (atom 0)] (set! cljs.core/compare-indexed (fn ([x y] (swap! calls inc) 17) ([x y len n] (throw 73)))) (try (and (= (compare (RedNode. 7 9 nil nil nil) [7 10]) 17) (= @calls 1)) (finally (set! cljs.core/compare-indexed saved))))}
#js {:id "rednode-comparable-rejection-format-order" :value (let [n (RedNode. 7 9 nil nil nil) saved str_ trace (atom [])] (set! str_ (fn [x] (swap! trace conj x) (js-obj "toString" (fn [] (swap! trace conj 17) "z")))) (try (try (compare n "x") false (catch :default e (and (= (.-message e) "Cannot compare z to z") (= @trace [n "x" 17 17])))) (finally (set! str_ saved))))}
])))
(set! *main-cli-fn* -main)
