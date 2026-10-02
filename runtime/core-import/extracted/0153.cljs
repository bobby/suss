;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype TransientVector [^:mutable cnt
                          ^:mutable shift
                          ^:mutable root
                          ^:mutable tail]
  ITransientCollection
  (-conj! [tcoll o]
    (if ^boolean (.-edit root)
      (if (< (- cnt (tail-off tcoll)) 32)
        (do (aset tail (bit-and cnt 0x01f) o)
            (set! cnt (inc cnt))
            tcoll)
        (let [tail-node (VectorNode. (.-edit root) tail)
              new-tail  (make-array 32)]
          (aset new-tail 0 o)
          (set! tail new-tail)
          (if (> (bit-shift-right-zero-fill cnt 5)
                 (bit-shift-left 1 shift))
            (let [new-root-array (make-array 32)
                  new-shift      (+ shift 5)]
              (aset new-root-array 0 root)
              (aset new-root-array 1 (new-path (.-edit root) shift tail-node))
              (set! root  (VectorNode. (.-edit root) new-root-array))
              (set! shift new-shift)
              (set! cnt   (inc cnt))
              tcoll)
            (let [new-root (tv-push-tail tcoll shift root tail-node)]
              (set! root new-root)
              (set! cnt  (inc cnt))
              tcoll))))
      (throw (js/Error. "conj! after persistent!"))))

  (-persistent! [tcoll]
    (if ^boolean (.-edit root)
      (do (set! (.-edit root) nil)
          (let [len (- cnt (tail-off tcoll))
                trimmed-tail (make-array len)]
            (array-copy tail 0 trimmed-tail 0 len)
            (PersistentVector. nil cnt shift root trimmed-tail nil)))
      (throw (js/Error. "persistent! called twice"))))

  ITransientAssociative
  (-assoc! [tcoll key val]
    (if (number? key)
      (-assoc-n! tcoll key val)
      (throw (js/Error. "TransientVector's key for assoc! must be a number."))))

  ITransientVector
  (-assoc-n! [tcoll n val]
    (if ^boolean (.-edit root)
      (cond
        (and (<= 0 n) (< n cnt))
        (if (<= (tail-off tcoll) n)
          (do (aset tail (bit-and n 0x01f) val)
              tcoll)
          (let [new-root
                ((fn go [level node]
                   (let [node (tv-ensure-editable (.-edit root) node)]
                     (if (zero? level)
                       (do (pv-aset node (bit-and n 0x01f) val)
                           node)
                       (let [subidx (bit-and (bit-shift-right-zero-fill n level)
                                             0x01f)]
                         (pv-aset node subidx
                                  (go (- level 5) (pv-aget node subidx)))
                         node))))
                 shift root)]
            (set! root new-root)
            tcoll))
        (== n cnt) (-conj! tcoll val)
        :else
        (throw
         (js/Error.
          (str_ "Index " n " out of bounds for TransientVector of length" cnt))))
      (throw (js/Error. "assoc! after persistent!"))))

  (-pop! [tcoll]
    (if ^boolean (.-edit root)
      (cond
        (zero? cnt) (throw (js/Error. "Can't pop empty vector"))
        (== 1 cnt)                       (do (set! cnt 0) tcoll)
        (pos? (bit-and (dec cnt) 0x01f)) (do (set! cnt (dec cnt)) tcoll)
        :else
        (let [new-tail (unchecked-editable-array-for tcoll (- cnt 2))
              new-root (let [nr (tv-pop-tail tcoll shift root)]
                         (if-not (nil? nr)
                           nr
                           (VectorNode. (.-edit root) (make-array 32))))]
          (if (and (< 5 shift) (nil? (pv-aget new-root 1)))
            (let [new-root (tv-ensure-editable (.-edit root) (pv-aget new-root 0))]
              (set! root  new-root)
              (set! shift (- shift 5))
              (set! cnt   (dec cnt))
              (set! tail  new-tail)
              tcoll)
            (do (set! root new-root)
                (set! cnt  (dec cnt))
                (set! tail new-tail)
                tcoll))))
      (throw (js/Error. "pop! after persistent!"))))

  ICounted
  (-count [coll]
    (if ^boolean (.-edit root)
      cnt
      (throw (js/Error. "count after persistent!"))))

  IIndexed
  (-nth [coll n]
    (if ^boolean (.-edit root)
      (aget (array-for coll n) (bit-and n 0x01f))
      (throw (js/Error. "nth after persistent!"))))

  (-nth [coll n not-found]
    (if (and (<= 0 n) (< n cnt))
      (-nth coll n)
      not-found))

  ILookup
  (-lookup [coll k] (-lookup coll k nil))

  (-lookup [coll k not-found]
    (cond
      (not ^boolean (.-edit root)) (throw (js/Error. "lookup after persistent!"))
      (number? k) (-nth coll k not-found)
      :else not-found))

  IFn
  (-invoke [coll k]
    (-lookup coll k))

  (-invoke [coll k not-found]
    (-lookup coll k not-found)))
