;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype BitmapIndexedNode [edit ^:mutable bitmap ^:mutable arr]
  Object
  (inode-assoc [inode shift hash key val added-leaf?]
    (let [bit (bitpos hash shift)
          idx (bitmap-indexed-node-index bitmap bit)]
      (if (zero? (bit-and bitmap bit))
        (let [n (bit-count bitmap)]
          (if (>= n 16)
            (let [nodes (make-array 32)
                  jdx   (mask hash shift)]
              (aset nodes jdx (.inode-assoc (.-EMPTY BitmapIndexedNode) (+ shift 5) hash key val added-leaf?))
              (loop [i 0 j 0]
                (if (< i 32)
                  (if (zero? (bit-and (bit-shift-right-zero-fill bitmap i) 1))
                    (recur (inc i) j)
                    (do (aset nodes i
                              (if-not (nil? (aget arr j))
                                (.inode-assoc (.-EMPTY BitmapIndexedNode)
                                              (+ shift 5) (cljs.core/hash (aget arr j)) (aget arr j) (aget arr (inc j)) added-leaf?)
                                (aget arr (inc j))))
                        (recur (inc i) (+ j 2))))))
              (ArrayNode. nil (inc n) nodes))
            (let [new-arr (make-array (* 2 (inc n)))]
              (array-copy arr 0 new-arr 0 (* 2 idx))
              (aset new-arr (* 2 idx) key)
              (aset new-arr (inc (* 2 idx)) val)
              (array-copy arr (* 2 idx) new-arr (* 2 (inc idx)) (* 2 (- n idx)))
              (set! (.-val added-leaf?) true)
              (BitmapIndexedNode. nil (bit-or bitmap bit) new-arr))))
        (let [key-or-nil  (aget arr (* 2 idx))
              val-or-node (aget arr (inc (* 2 idx)))]
          (cond (nil? key-or-nil)
                (let [n (.inode-assoc val-or-node (+ shift 5) hash key val added-leaf?)]
                  (if (identical? n val-or-node)
                    inode
                    (BitmapIndexedNode. nil bitmap (clone-and-set arr (inc (* 2 idx)) n))))

                (key-test key key-or-nil)
                (if (identical? val val-or-node)
                  inode
                  (BitmapIndexedNode. nil bitmap (clone-and-set arr (inc (* 2 idx)) val)))

                :else
                (do (set! (.-val added-leaf?) true)
                    (BitmapIndexedNode. nil bitmap
                                        (clone-and-set arr (* 2 idx) nil (inc (* 2 idx))
                                                       (create-node (+ shift 5) key-or-nil val-or-node hash key val)))))))))

  (inode-without [inode shift hash key]
    (let [bit (bitpos hash shift)]
      (if (zero? (bit-and bitmap bit))
        inode
        (let [idx         (bitmap-indexed-node-index bitmap bit)
              key-or-nil  (aget arr (* 2 idx))
              val-or-node (aget arr (inc (* 2 idx)))]
          (cond (nil? key-or-nil)
                (let [n (.inode-without val-or-node (+ shift 5) hash key)]
                  (cond (identical? n val-or-node) inode
                        (not (nil? n)) (BitmapIndexedNode. nil bitmap (clone-and-set arr (inc (* 2 idx)) n))
                        (== bitmap bit) nil
                        :else (BitmapIndexedNode. nil (bit-xor bitmap bit) (remove-pair arr idx))))
                (key-test key key-or-nil)
                (if (== bitmap bit)
                  nil
                  (BitmapIndexedNode. nil (bit-xor bitmap bit) (remove-pair arr idx)))
                :else inode)))))

  (inode-lookup [inode shift hash key not-found]
    (let [bit (bitpos hash shift)]
      (if (zero? (bit-and bitmap bit))
        not-found
        (let [idx         (bitmap-indexed-node-index bitmap bit)
              key-or-nil  (aget arr (* 2 idx))
              val-or-node (aget arr (inc (* 2 idx)))]
          (cond (nil? key-or-nil)  (.inode-lookup val-or-node (+ shift 5) hash key not-found)
                (key-test key key-or-nil) val-or-node
                :else not-found)))))

  (inode-find [inode shift hash key not-found]
    (let [bit (bitpos hash shift)]
      (if (zero? (bit-and bitmap bit))
        not-found
        (let [idx         (bitmap-indexed-node-index bitmap bit)
              key-or-nil  (aget arr (* 2 idx))
              val-or-node (aget arr (inc (* 2 idx)))]
          (cond (nil? key-or-nil) (.inode-find val-or-node (+ shift 5) hash key not-found)
                (key-test key key-or-nil)          (MapEntry. key-or-nil val-or-node nil)
                :else not-found)))))

  (inode-seq [inode]
    (create-inode-seq arr))

  (ensure-editable [inode e]
    (if (identical? e edit)
      inode
      (let [n       (bit-count bitmap)
            new-arr (make-array (if (neg? n) 4 (* 2 (inc n))))]
        (array-copy arr 0 new-arr 0 (* 2 n))
        (BitmapIndexedNode. e bitmap new-arr))))

  (edit-and-remove-pair [inode e bit i]
    (if (== bitmap bit)
      nil
      (let [editable (.ensure-editable inode e)
            earr     (.-arr editable)
            len      (alength earr)]
        (set! (.-bitmap editable) (bit-xor bit (.-bitmap editable)))
        (array-copy earr (* 2 (inc i))
                    earr (* 2 i)
                    (- len (* 2 (inc i))))
        (aset earr (- len 2) nil)
        (aset earr (dec len) nil)
        editable)))

  (inode-assoc! [inode edit shift hash key val added-leaf?]
    (let [bit (bitpos hash shift)
          idx (bitmap-indexed-node-index bitmap bit)]
      (if (zero? (bit-and bitmap bit))
        (let [n (bit-count bitmap)]
          (cond
            (< (* 2 n) (alength arr))
            (let [editable (.ensure-editable inode edit)
                  earr     (.-arr editable)]
              (set! (.-val added-leaf?) true)
              (array-copy-downward earr (* 2 idx)
                                   earr (* 2 (inc idx))
                                   (* 2 (- n idx)))
              (aset earr (* 2 idx) key)
              (aset earr (inc (* 2 idx)) val)
              (set! (.-bitmap editable) (bit-or (.-bitmap editable) bit))
              editable)

            (>= n 16)
            (let [nodes (make-array 32)
                  jdx   (mask hash shift)]
              (aset nodes jdx (.inode-assoc! (.-EMPTY BitmapIndexedNode) edit (+ shift 5) hash key val added-leaf?))
              (loop [i 0 j 0]
                (if (< i 32)
                  (if (zero? (bit-and (bit-shift-right-zero-fill bitmap i) 1))
                    (recur (inc i) j)
                    (do (aset nodes i
                              (if-not (nil? (aget arr j))
                                (.inode-assoc! (.-EMPTY BitmapIndexedNode)
                                               edit (+ shift 5) (cljs.core/hash (aget arr j)) (aget arr j) (aget arr (inc j)) added-leaf?)
                                (aget arr (inc j))))
                        (recur (inc i) (+ j 2))))))
              (ArrayNode. edit (inc n) nodes))

            :else
            (let [new-arr (make-array (* 2 (+ n 4)))]
              (array-copy arr 0 new-arr 0 (* 2 idx))
              (aset new-arr (* 2 idx) key)
              (aset new-arr (inc (* 2 idx)) val)
              (array-copy arr (* 2 idx) new-arr (* 2 (inc idx)) (* 2 (- n idx)))
              (set! (.-val added-leaf?) true)
              (let [editable (.ensure-editable inode edit)]
                (set! (.-arr editable) new-arr)
                (set! (.-bitmap editable) (bit-or (.-bitmap editable) bit))
                editable))))
        (let [key-or-nil  (aget arr (* 2 idx))
              val-or-node (aget arr (inc (* 2 idx)))]
          (cond (nil? key-or-nil)
                (let [n (.inode-assoc! val-or-node edit (+ shift 5) hash key val added-leaf?)]
                  (if (identical? n val-or-node)
                    inode
                    (edit-and-set inode edit (inc (* 2 idx)) n)))

                (key-test key key-or-nil)
                (if (identical? val val-or-node)
                  inode
                  (edit-and-set inode edit (inc (* 2 idx)) val))

                :else
                (do (set! (.-val added-leaf?) true)
                    (edit-and-set inode edit (* 2 idx) nil (inc (* 2 idx))
                                  (create-node edit (+ shift 5) key-or-nil val-or-node hash key val))))))))

  (inode-without! [inode edit shift hash key removed-leaf?]
    (let [bit (bitpos hash shift)]
      (if (zero? (bit-and bitmap bit))
        inode
        (let [idx         (bitmap-indexed-node-index bitmap bit)
              key-or-nil  (aget arr (* 2 idx))
              val-or-node (aget arr (inc (* 2 idx)))]
          (cond (nil? key-or-nil)
                (let [n (.inode-without! val-or-node edit (+ shift 5) hash key removed-leaf?)]
                  (cond (identical? n val-or-node) inode
                        (not (nil? n)) (edit-and-set inode edit (inc (* 2 idx)) n)
                        (== bitmap bit) nil
                        :else (.edit-and-remove-pair inode edit bit idx)))
                (key-test key key-or-nil)
                (do (set! (.-val removed-leaf?) true)
                    (.edit-and-remove-pair inode edit bit idx))
                :else inode)))))

  (kv-reduce [inode f init]
    (inode-kv-reduce arr f init))

  IIterable
  (-iterator [coll]
    (NodeIterator. arr 0 nil nil)))
