//! Original private boundary schema for the retained persistent-vector layout.
//! Prepared against the first Runtime core catalog, then captured before user
//! initializers. No public core dispatcher is used to inspect boundary values.
pub(super) const SOURCE: &str = r#"
(let [identical suss.core/identical?
      allocate suss.core/make-array
      array suss.core/array
      get suss.core/aget
      put suss.core/aset
      array-length suss.core/alength
      array? suss.core/array?
      vector-class suss.core/PersistentVector
      subvector-class suss.core/Subvec
      node-class suss.core/VectorNode
      empty-root (.-EMPTY_NODE suss.core/PersistentVector)
      checked-array
      (fn [value length]
        (if (array? value)
          (if (identical (array-length value) length) value
            (throw "WIT list requires valid persistent vector array lengths"))
          (throw "WIT list requires persistent vector array storage")))
      checked-node
      (fn [node]
        (if (suss.core/instance? node-class node)
          (do (checked-array (.-arr node) 32) node)
          (throw "WIT list requires a valid persistent vector trie")))
      bounded-length
      (fn [length]
        (if (identical length (suss.core/bit-or length 0))
          (if (suss.core/< length 0)
            (throw "WIT list requires a nonnegative vector length")
            (if (suss.core/> length 1000000)
              (throw "WIT list exceeds the bounded source array capacity") length))
          (throw "WIT list requires an integral vector length")))
      normalize
      (fn [value]
        (loop [value value offset 0 requested nil depth 0]
          (if (suss.core/> depth 64)
            (throw "WIT list exceeds the bounded subvector view depth")
            (if (suss.core/instance? vector-class value)
              (let [count (bounded-length (.-cnt value))
                    length (if (identical nil requested) count requested)
                    tail-start (if (suss.core/<= count 32) 0
                                 (suss.core/bit-and (suss.core/- count 1) -32))
                    tail (checked-array (.-tail value) (suss.core/- count tail-start))
                    root (checked-node (.-root value))]
                (if (suss.core/> (suss.core/+ offset length) count)
                  (throw "WIT list subvector range exceeds its vector")
                  (array value offset length)))
              (if (suss.core/instance? subvector-class value)
                (let [start (bounded-length (.-start value))
                      end (bounded-length (.-end value))
                      length (bounded-length (suss.core/- end start))
                      requested (if (identical nil requested) length requested)]
                  (if (suss.core/> (suss.core/+ offset requested) length)
                    (throw "WIT list subvector range exceeds its view")
                    (recur (.-v value) (suss.core/+ start offset) requested
                           (suss.core/+ depth 1))))
                (throw "WIT list requires a persistent vector"))))))
      construct
      (fn [values]
        (let [count (bounded-length (array-length values))
              tail-start (if (suss.core/<= count 32) 0
                           (suss.core/bit-and (suss.core/- count 1) -32))
              tail-length (suss.core/- count tail-start)
              tail (allocate tail-length)
              leaf-count (suss.core/bit-shift-right tail-start 5)
              leaves (allocate leaf-count)]
          (loop [index 0]
            (if (suss.core/< index tail-length)
              (do (put tail index (get values (suss.core/+ tail-start index)))
                  (recur (suss.core/+ index 1))) nil))
          (loop [leaf 0]
            (if (suss.core/< leaf leaf-count)
              (let [items (allocate 32)]
                (loop [index 0]
                  (if (suss.core/< index 32)
                    (do (put items index
                             (get values (suss.core/+ (suss.core/* leaf 32) index)))
                        (recur (suss.core/+ index 1))) nil))
                (put leaves leaf (new node-class nil items))
                (recur (suss.core/+ leaf 1))) nil))
          (if (identical leaf-count 0)
            (new vector-class nil count 5 empty-root tail nil)
            (loop [nodes leaves node-count leaf-count shift 5]
              (if (suss.core/<= node-count 32)
                (let [items (allocate 32)]
                  (loop [index 0]
                    (if (suss.core/< index node-count)
                      (do (put items index (get nodes index))
                          (recur (suss.core/+ index 1))) nil))
                  (new vector-class nil count shift (new node-class nil items) tail nil))
                (let [parent-count (suss.core/bit-shift-right (suss.core/+ node-count 31) 5)
                      parents (allocate parent-count)]
                  (loop [parent 0]
                    (if (suss.core/< parent parent-count)
                      (let [items (allocate 32)]
                        (loop [index 0]
                          (let [child (suss.core/+ (suss.core/* parent 32) index)]
                            (if (if (suss.core/< index 32) (suss.core/< child node-count) false)
                              (do (put items index (get nodes child))
                                  (recur (suss.core/+ index 1))) nil)))
                        (put parents parent (new node-class nil items))
                        (recur (suss.core/+ parent 1))) nil))
                  (recur parents parent-count (suss.core/+ shift 5))))))))
      indexed
      (fn [value index]
        (let [parts value
              vector (get parts 0)
              offset (get parts 1)
              length (get parts 2)
              index (bounded-length index)]
          (if (suss.core/>= index length)
            (throw "WIT list element index exceeds its vector")
            (let [index (suss.core/+ offset index)
                  count (.-cnt vector)
                  tail-start (if (suss.core/<= count 32) 0
                               (suss.core/bit-and (suss.core/- count 1) -32))]
              (if (suss.core/>= index tail-start)
                (get (.-tail vector) (suss.core/bit-and index 31))
                (let [shift (.-shift vector)]
                  (if (if (identical shift 5) true
                        (if (identical shift 10) true
                          (if (identical shift 15) true
                            (if (identical shift 20) true
                              (if (identical shift 25) true (identical shift 30))))))
                    (loop [node (.-root vector) level shift]
                      (let [node (checked-node node)]
                        (if (identical level 0)
                          (get (.-arr node) (suss.core/bit-and index 31))
                          (recur (get (.-arr node)
                                      (suss.core/bit-and (suss.core/bit-shift-right index level) 31))
                                 (suss.core/- level 5)))))
                    (throw "WIT list requires a valid vector shift"))))))))]
  (fn [operation value index]
    (if (identical operation 0) (construct value)
      (if (identical operation 1) (normalize value)
        (if (identical operation 3) (get value 2)
          (indexed value index))))))
"#;
