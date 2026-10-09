(def fast-path-protocols
  "protocol fqn -> [partition number, bit]"
  (zipmap (map #(symbol "cljs.core" (core/str %))
               '[IFn ICounted IEmptyableCollection ICollection IIndexed ASeq ISeq INext
                 ILookup IAssociative IMap IMapEntry ISet IStack IVector IDeref
                 IDerefWithTimeout IMeta IWithMeta IReduce IKVReduce IEquiv IHash
                 ISeqable ISequential IList IRecord IReversible ISorted IPrintWithWriter IWriter
                 IPrintWithWriter IPending IWatchable IEditableCollection ITransientCollection
                 ITransientAssociative ITransientMap ITransientVector ITransientSet
                 IMultiFn IChunkedSeq IChunkedNext IComparable INamed ICloneable IAtom
                 IReset ISwap IIterable IDrop])
          (iterate (core/fn [[p b]]
                     (if (core/== 2147483648 b)
                       [(core/inc p) 1]
                       [p #?(:clj  (core/bit-shift-left b 1)
                             :cljs (core/* 2 b))]))
                   [0 1])))
