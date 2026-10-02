;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype PersistentHashMap [meta cnt root ^boolean has-nil? nil-val ^:mutable __hash]
  Object
  (toString [coll]
    (pr-str* coll))
  (equiv [this other]
    (-equiv this other))
  (keys [coll]
    (es6-iterator (keys coll)))
  (entries [coll]
    (es6-entries-iterator (seq coll)))
  (values [coll]
    (es6-iterator (vals coll)))
  (has [coll k]
    (contains? coll k))
  (get [coll k not-found]
    (-lookup coll k not-found))
  (forEach [coll f]
    (doseq [[k v] coll]
      (f v k)))

  ICloneable
  (-clone [_] (PersistentHashMap. meta cnt root has-nil? nil-val __hash))

  IIterable
  (-iterator [coll]
    (let [root-iter (if ^boolean root (-iterator root) (nil-iter))]
      (if has-nil?
        (HashMapIter. nil-val root-iter false)
        root-iter)))

  IWithMeta
  (-with-meta [coll new-meta]
    (if (identical? new-meta meta)
      coll
      (PersistentHashMap. new-meta cnt root has-nil? nil-val __hash)))

  IMeta
  (-meta [coll] meta)

  ICollection
  (-conj [coll entry]
    (if (vector? entry)
      (-assoc coll (-nth entry 0) (-nth entry 1))
      (loop [ret coll es (seq entry)]
        (if (nil? es)
          ret
          (let [e (first es)]
            (if (vector? e)
              (recur (-assoc ret (-nth e 0) (-nth e 1))
                     (next es))
              (throw (js/Error. "conj on a map takes map entries or seqables of map entries"))))))))

  IEmptyableCollection
  (-empty [coll] (-with-meta (.-EMPTY PersistentHashMap) meta))

  IEquiv
  (-equiv [coll other] (equiv-map coll other))

  IHash
  (-hash [coll] (caching-hash coll hash-unordered-coll __hash))

  ISeqable
  (-seq [coll]
    (when (pos? cnt)
      (let [s (if-not (nil? root) (.inode-seq root))]
        (if has-nil?
          (cons (MapEntry. nil nil-val nil) s)
          s))))

  ICounted
  (-count [coll] cnt)

  ILookup
  (-lookup [coll k]
    (-lookup coll k nil))

  (-lookup [coll k not-found]
    (cond (nil? k)    (if has-nil?
                        nil-val
                        not-found)
          (nil? root) not-found
          :else       (.inode-lookup root 0 (hash k) k not-found)))

  IAssociative
  (-assoc [coll k v]
    (if (nil? k)
      (if (and has-nil? (identical? v nil-val))
        coll
        (PersistentHashMap. meta (if has-nil? cnt (inc cnt)) root true v nil))
      (let [added-leaf? (Box. false)
            new-root    (-> (if (nil? root)
                              (.-EMPTY BitmapIndexedNode)
                              root)
                            (.inode-assoc 0 (hash k) k v added-leaf?))]
        (if (identical? new-root root)
          coll
          (PersistentHashMap. meta (if ^boolean (.-val added-leaf?) (inc cnt) cnt) new-root has-nil? nil-val nil)))))

  (-contains-key? [coll k]
    (cond (nil? k)    has-nil?
          (nil? root) false
          :else       (not (identical? (.inode-lookup root 0 (hash k) k lookup-sentinel)
                                       lookup-sentinel))))

  IFind
  (-find [coll k]
    (cond
      (nil? k) (when has-nil? (MapEntry. nil nil-val nil))
      (nil? root) nil
      :else (.inode-find root 0 (hash k) k nil)))

  IMap
  (-dissoc [coll k]
    (cond (nil? k)    (if has-nil?
                        (PersistentHashMap. meta (dec cnt) root false nil nil)
                        coll)
          (nil? root) coll
          :else
          (let [new-root (.inode-without root 0 (hash k) k)]
            (if (identical? new-root root)
              coll
              (PersistentHashMap. meta (dec cnt) new-root has-nil? nil-val nil)))))

  IKVReduce
  (-kv-reduce [coll f init]
    (let [init (if has-nil? (f init nil nil-val) init)]
      (cond
        (reduced? init)          @init
        (not (nil? root)) (unreduced (.kv-reduce root f init))
        :else                    init)))

  IFn
  (-invoke [coll k]
    (-lookup coll k))

  (-invoke [coll k not-found]
    (-lookup coll k not-found))

  IEditableCollection
  (-as-transient [coll]
    (TransientHashMap. (js-obj) root cnt has-nil? nil-val)))
