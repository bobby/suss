(core/defmulti ^:private extend-prefix (core/fn [tsym sym] (core/-> tsym meta :extend)))
