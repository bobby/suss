(core/defn- emit-defrecord
  "Do not use this directly - use defrecord"
  [env tagname rname fields impls]
  (core/let [hinted-fields fields
             fields (vec (map #(with-meta % nil) fields))
             base-fields fields
             pr-open (core/str "#" #?(:clj  (.getNamespace rname)
                                      :cljs (namespace rname))
                               "." #?(:clj  (.getName rname)
                                      :cljs (name rname))
                               "{")
             fields (conj fields '__meta '__extmap (with-meta '__hash {:mutable true}))]
    (core/let [gs (gensym)
               ksym (gensym "k")
               impls (concat
                       impls
                       ['IRecord
                        'ICloneable
                        `(~'-clone [this#] (new ~tagname ~@fields))
                        'IHash
                        `(~'-hash [this#]
                           (caching-hash this#
                             (fn [coll#]
                               (bit-xor
                                 ~(hash (core/-> rname comp/munge core/str))
                                 (hash-unordered-coll coll#)))
                             ~'__hash))
                        'IEquiv
                        (core/let [this (gensym 'this) other (gensym 'other)]
                          `(~'-equiv [~this ~other]
                             (and (some? ~other)
                                  (identical? (.-constructor ~this)
                                              (.-constructor ~other))
                                  ~@(map (core/fn [field]
                                           `(= (.. ~this ~(to-property field))
                                               (.. ~(with-meta other {:tag tagname}) ~(to-property field))))
                                         base-fields)
                                  (= (.-__extmap ~this)
                                     (.-__extmap ~(with-meta other {:tag tagname}))))))
                        'IMeta
                        `(~'-meta [this#] ~'__meta)
                        'IWithMeta
                        `(~'-with-meta [this# ~gs] (new ~tagname ~@(replace {'__meta gs} fields)))
                        'ILookup
                        `(~'-lookup [this# k#] (-lookup this# k# nil))
                        `(~'-lookup [this# ~ksym else#]
                           (case ~ksym
                             ~@(mapcat (core/fn [f] [(keyword f) f]) base-fields)
                             (cljs.core/get ~'__extmap ~ksym else#)))
                        'ICounted
                        `(~'-count [this#] (+ ~(count base-fields) (count ~'__extmap)))
                        'ICollection
                        `(~'-conj [this# entry#]
                           (if (vector? entry#)
                             (-assoc this# (-nth entry# 0) (-nth entry# 1))
                             (reduce -conj
                               this#
                               entry#)))
                        'IAssociative
                        `(~'-contains-key? [this# ~ksym]
                           ~(if (seq base-fields)
                             `(case ~ksym
                                (~@(map keyword base-fields)) true
                                (cljs.core/contains? ~'__extmap ~ksym))
                             `(cljs.core/contains? ~'__extmap ~ksym)))
                        `(~'-assoc [this# k# ~gs]
                           (condp keyword-identical? k#
                             ~@(mapcat (core/fn [fld]
                                         [(keyword fld) (list* `new tagname (replace {fld gs '__hash nil} fields))])
                                 base-fields)
                             (new ~tagname ~@(remove #{'__extmap '__hash} fields) (assoc ~'__extmap k# ~gs) nil)))
                        'IMap
                        `(~'-dissoc [this# k#] (if (contains? #{~@(map keyword base-fields)} k#)
                                                 (dissoc (-with-meta (into {} this#) ~'__meta) k#)
                                                 (new ~tagname ~@(remove #{'__extmap '__hash} fields)
                                                   (not-empty (dissoc ~'__extmap k#))
                                                   nil)))
                        'ISeqable
                        `(~'-seq [this#] (seq (concat [~@(map #(core/list 'cljs.core/MapEntry. (keyword %) % nil) base-fields)]
                                                ~'__extmap)))

                        'IIterable
                        `(~'-iterator [~gs]
                          (RecordIter. 0 ~gs ~(count base-fields) [~@(map keyword base-fields)] (if ~'__extmap
                                                                                                  (-iterator ~'__extmap)
                                                                                                  (core/nil-iter))))

                        'IPrintWithWriter
                        `(~'-pr-writer [this# writer# opts#]
                           (let [pr-pair# (fn [keyval#] (pr-sequential-writer writer# (~'js* "cljs.core.pr_writer") "" " " "" opts# keyval#))]
                             (pr-sequential-writer
                               writer# pr-pair# ~pr-open ", " "}" opts#
                               (concat [~@(map #(core/list `vector (keyword %) %) base-fields)]
                                 ~'__extmap))))
                        'IKVReduce
                        `(~'-kv-reduce [this# f# init#]
                           (reduce (fn [ret# [k# v#]] (f# ret# k# v#)) init# this#))
                        ])
               [fpps pmasks] (prepare-protocol-masks env impls)
               protocols (collect-protocols impls env)
               tagname (vary-meta tagname assoc
                         :protocols protocols
                         :skip-protocol-flag fpps)]
      `(do
         (~'defrecord* ~tagname ~hinted-fields ~pmasks
           (extend-type ~tagname ~@(dt->et tagname impls fields true)))))))
