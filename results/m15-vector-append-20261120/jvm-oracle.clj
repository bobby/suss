(assert (= "1.12.6" (clojure-version)))
(prn 
          (mapv (fn [n]
            (let [before (vec (range n))
                  after (persistent! (reduce conj! (transient before) (range n (+ n 70))))]
              [n (count after) (nth after n) (peek after)
               (= before (vec (range n))) (= after (vec (range (+ n 70))))]))
            [0 1 30 31 32 33 63 64 65 1023 1024 1055 1056 1057]))
(deftype Tap [events]
 clojure.lang.ITransientCollection
 (conj [this x] (swap! events conj x) this)
 (persistent [_] @events))
(deftype NilTap [events]
 clojure.lang.ITransientCollection
 (conj [_ x] (swap! events conj x) nil)
 (persistent [_] @events))

(prn 
          [(let [events (atom []) t (Tap. events)
                 result (conj! (do (swap! events conj :receiver) t)
                               (do (swap! events conj :value) 7))]
             [(identical? t result) (persistent! t)])
           (let [events (atom [])]
             [(nil? (conj! (NilTap. events) 9)) @events])
           (let [events (atom []) t (transient [1]) p (persistent! t)
                 result (try (conj! (do (swap! events conj :receiver) t)
                                    (do (swap! events conj :value) 9))
                             :missing-error (catch Throwable e :threw))]
             [result p @events])
           (let [events (atom [])
                 result (try (conj! (do (swap! events conj :receiver) nil)
                                    (do (swap! events conj :value) 9))
                             :missing-error (catch Throwable e :threw))]
             [result @events])])

          (def retained-tail-values
            (let [a (atom 1) f (fn [] (swap! a inc))
                  t (transient [])]
              (reduce conj! t [a f nil false :tag "text" 1.5 9223372036854775807])
              [a f (persistent! t)]))
(System/gc)
(prn 
          (let [[a f p] retained-tail-values]
            [(identical? a (nth p 0)) (identical? f (nth p 1))
             ((nth p 1)) @a (subvec p 2)]))
