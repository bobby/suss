(core/defmacro reify
  "reify creates an object implementing a protocol.
  reify is a macro with the following structure:

 (reify options* specs*)

  Currently there are no options.

  Each spec consists of the protocol name followed by zero
  or more method bodies:

  protocol
  (methodName [args+] body)*

  Methods should be supplied for all methods of the desired
  protocol(s). You can also define overrides for Object methods. Note that
  the first parameter must be supplied to correspond to the target object
  ('this' in JavaScript parlance). Note also that recur calls
  to the method head should *not* pass the target object, it will be supplied
  automatically and can not be substituted.

  recur works to method heads The method bodies of reify are lexical
  closures, and can refer to the surrounding local scope:

  (str (let [f \"foo\"]
       (reify Object
         (toString [this] f))))
  == \"foo\"

  (seq (let [f \"foo\"]
       (reify ISeqable
         (-seq [this] (seq f)))))
  == (\"f\" \"o\" \"o\"))

  reify always implements IMeta and IWithMeta and transfers meta
  data of the form to the created object.

  (meta ^{:k :v} (reify Object (toString [this] \"foo\")))
  == {:k :v}"
  [& impls]
  (core/let [t        (with-meta
                        (gensym
                          (core/str "t_reify_"
                            (string/replace (core/str (munge ana/*cljs-ns*)) "." "$")))
                        {:anonymous true})
             meta-sym (gensym "meta")
             this-sym (gensym "_")
             locals   (keys (:locals &env))
             ns       (core/-> &env :ns :name)
             munge    comp/munge]
    `(do
       (when-not (exists? ~(symbol (core/str ns) (core/str t)))
         (deftype ~t [~@locals ~meta-sym]
           IWithMeta
           (~'-with-meta [~this-sym ~meta-sym]
             (new ~t ~@locals ~meta-sym))
           IMeta
           (~'-meta [~this-sym] ~meta-sym)
           ~@impls))
       (new ~t ~@locals
         ;; if the form meta is empty, emit nil
         ~(core/let [form-meta (ana/elide-reader-meta (meta &form))]
            (core/when-not (empty? form-meta)
              form-meta))))))
