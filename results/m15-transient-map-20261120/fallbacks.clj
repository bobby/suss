(assert (= "1.12.6" (clojure-version)))
(prn [(= (hash -51283) (hash 37831)) (= -51283 37831)
      (get (assoc {-51283 :old} 37831 :new) -51283)
      (get (assoc {-51283 :old} 37831 :new) 37831)])
(deftype CustomEdit [events]
 clojure.lang.ITransientAssociative
 (assoc [_ k v] (swap! events conj [:assoc k v]) nil)
 (conj [_ v] (swap! events conj [:conj v]) nil)
 (persistent [_] @events))
(prn (let [events (atom []) t (CustomEdit. events)]
 [(assoc! (do (swap! events conj :receiver) t)
          (do (swap! events conj :key) 1)
          (do (swap! events conj :value) 2))
  (conj! t [1 2]) @events]))
