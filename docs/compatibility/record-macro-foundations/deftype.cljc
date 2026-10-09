(core/defmacro deftype
  "(deftype name [fields*]  options* specs*)

  Currently there are no options.

  Each spec consists of a protocol or interface name followed by zero
  or more method bodies:

  protocol-or-Object
  (methodName [args*] body)*

  The type will have the (by default, immutable) fields named by
  fields, which can have type hints. Protocols and methods
  are optional. The only methods that can be supplied are those
  declared in the protocols/interfaces.  Note that method bodies are
  not closures, the local environment includes only the named fields,
  and those fields can be accessed directly. Fields can be qualified
  with the metadata :mutable true at which point (set! afield aval) will be
  supported in method bodies. Note well that mutable fields are extremely
  difficult to use correctly, and are present only to facilitate the building
  of higherlevel constructs, such as ClojureScript's reference types, in
  ClojureScript itself. They are for experts only - if the semantics and
  implications of :mutable are not immediately apparent to you, you should not
  be using them.

  Method definitions take the form:

  (methodname [args*] body)

  The argument and return types can be hinted on the arg and
  methodname symbols. If not supplied, they will be inferred, so type
  hints should be reserved for disambiguation.

  Methods should be supplied for all methods of the desired
  protocol(s). You can also define overrides for methods of Object. Note that
  a parameter must be supplied to correspond to the target object
  ('this' in JavaScript parlance). Note also that recur calls to the method
  head should *not* pass the target object, it will be supplied
  automatically and can not be substituted.

  In the method bodies, the (unqualified) name can be used to name the
  class (for calls to new, instance? etc).

  One constructor will be defined, taking the designated fields.  Note
  that the field names __meta and __extmap are currently reserved and
  should not be used when defining your own types.

  Given (deftype TypeName ...), a factory function called ->TypeName
  will be defined, taking positional parameters for the fields"
  [t fields & impls]
  (validate-fields "deftype" t fields)
  (core/let [env &env
             v (cljs.analyzer/resolve-var (dissoc env :locals) t)
             r (:name v)
             [fpps pmasks] (prepare-protocol-masks env impls)
             protocols (collect-protocols impls env)
             t (vary-meta t assoc
                 :protocols protocols
                 :skip-protocol-flag fpps)]
    `(do
       (deftype* ~t ~fields ~pmasks
         ~(if (seq impls)
            `(extend-type ~t ~@(dt->et t impls fields))))
       ;; don't emit static basis method w/ reify
       ;; nor for core types
       ~@(core/when-not (core/or (string/starts-with? (name t) "t_reify")
                                 (= 'cljs.core (:ns v)))
           [`(set! (.-getBasis ~t) (fn [] '[~@fields]))])
       (set! (.-cljs$lang$type ~t) true)
       (set! (.-cljs$lang$ctorStr ~t) ~(core/str r))
       (set! (.-cljs$lang$ctorPrWriter ~t) (fn [this# writer# opt#] (-write writer# ~(core/str r))))

       ~(build-positional-factory t r fields)
       ~t)))
