;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype TransientHashMap [^:mutable ^boolean edit
                           ^:mutable root
                           ^:mutable count
                           ^:mutable ^boolean has-nil?
                           ^:mutable nil-val]
  Object
  (conj! [tcoll o]
    (if edit
      (cond
        (map-entry? o)
        (.assoc! tcoll (key o) (val o))

        (vector? o)
        (.assoc! tcoll (o 0) (o 1))

        :else
        (loop [es (seq o) tcoll tcoll]
          (if-let [e (first es)]
            (recur (next es)
                   (.assoc! tcoll (key e) (val e)))
            tcoll)))
      (throw (js/Error. "conj! after persistent"))))

  (assoc! [tcoll k v]
    (if edit
      (if (nil? k)
        (do (if (identical? nil-val v)
              nil
              (set! nil-val v))
            (if has-nil?
              nil
              (do (set! count (inc count))
                  (set! has-nil? true)))
            tcoll)
        (let [added-leaf? (Box. false)
              node        (-> (if (nil? root)
                                (.-EMPTY BitmapIndexedNode)
                                root)
                              (.inode-assoc! edit 0 (hash k) k v added-leaf?))]
          (if (identical? node root)
            nil
            (set! root node))
          ;; FIXME: can we figure out something better here?
          (if ^boolean (.-val added-leaf?)
            (set! count (inc count)))
          tcoll))
      (throw (js/Error. "assoc! after persistent!"))))

  (without! [tcoll k]
    (if edit
      (if (nil? k)
        (if has-nil?
          (do (set! has-nil? false)
              (set! nil-val nil)
              (set! count (dec count))
              tcoll)
          tcoll)
        (if (nil? root)
          tcoll
          (let [removed-leaf? (Box. false)
                node (.inode-without! root edit 0 (hash k) k removed-leaf?)]
            (if (identical? node root)
              nil
              (set! root node))
            ;; FIXME: can we figure out something better here?
            (if ^boolean (.-val removed-leaf?)
              (set! count (dec count)))
            tcoll)))
      (throw (js/Error. "dissoc! after persistent!"))))

  (persistent! [tcoll]
    (if edit
      (do (set! edit nil)
          (PersistentHashMap. nil count root has-nil? nil-val nil))
      (throw (js/Error. "persistent! called twice"))))

  ICounted
  (-count [coll]
    (if edit
      count
      (throw (js/Error. "count after persistent!"))))

  ILookup
  (-lookup [tcoll k]
    (if (nil? k)
      (if has-nil?
        nil-val)
      (if (nil? root)
        nil
        (.inode-lookup root 0 (hash k) k))))

  (-lookup [tcoll k not-found]
    (if (nil? k)
      (if has-nil?
        nil-val
        not-found)
      (if (nil? root)
        not-found
        (.inode-lookup root 0 (hash k) k not-found))))

  ITransientCollection
  (-conj! [tcoll val] (.conj! tcoll val))

  (-persistent! [tcoll] (.persistent! tcoll))

  ITransientAssociative
  (-assoc! [tcoll key val] (.assoc! tcoll key val))

  ITransientMap
  (-dissoc! [tcoll key] (.without! tcoll key))

  IFn
  (-invoke [tcoll key]
    (-lookup tcoll key))
  (-invoke [tcoll key not-found]
    (-lookup tcoll key not-found)))
