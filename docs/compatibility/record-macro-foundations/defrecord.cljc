(core/defmacro defrecord
  "(defrecord name [fields*]  options* specs*)

  Currently there are no options.

  Each spec consists of a protocol or interface name followed by zero
  or more method bodies:

  protocol-or-Object
  (methodName [args*] body)*

  The record will have the (immutable) fields named by
  fields, which can have type hints. Protocols and methods
  are optional. The only methods that can be supplied are those
  declared in the protocols.  Note that method bodies are
  not closures, the local environment includes only the named fields,
  and those fields can be accessed directly.

  Method definitions take the form:

  (methodname [args*] body)

  The argument and return types can be hinted on the arg and
  methodname symbols. If not supplied, they will be inferred, so type
  hints should be reserved for disambiguation.

  Methods should be supplied for all methods of the desired
  protocol(s). You can also define overrides for
  methods of Object. Note that a parameter must be supplied to
  correspond to the target object ('this' in JavaScript parlance). Note also
  that recur calls to the method head should *not* pass the target object, it
  will be supplied automatically and can not be substituted.

  In the method bodies, the (unqualified) name can be used to name the
  class (for calls to new, instance? etc).

  The type will have implementations of several ClojureScript
  protocol generated automatically: IMeta/IWithMeta (metadata support) and
  IMap, etc.

  In addition, defrecord will define type-and-value-based =,
  and will define ClojureScript IHash and IEquiv.

  Two constructors will be defined, one taking the designated fields
  followed by a metadata map (nil for none) and an extension field
  map (nil for none), and one taking only the fields (using nil for
  meta and extension fields). Note that the field names __meta
  and __extmap are currently reserved and should not be used when
  defining your own records.

  Given (defrecord TypeName ...), two factory functions will be
  defined: ->TypeName, taking positional parameters for the fields,
  and map->TypeName, taking a map of keywords to field values."
  [rsym fields & impls]
  (validate-fields "defrecord" rsym fields)
  (core/let [rsym (vary-meta rsym assoc :internal-ctor true)
             r    (vary-meta
                    (:name (cljs.analyzer/resolve-var (dissoc &env :locals) rsym))
                    assoc :internal-ctor true)]
    `(let []
       ~(emit-defrecord &env rsym r fields impls)
       (set! (.-getBasis ~r) (fn [] '[~@fields]))
       (set! (.-cljs$lang$type ~r) true)
       (set! (.-cljs$lang$ctorPrSeq ~r) (fn [this#] (cljs.core/list ~(core/str r))))
       (set! (.-cljs$lang$ctorPrWriter ~r) (fn [this# writer#] (-write writer# ~(core/str r))))
       ~(build-positional-factory rsym r fields)
       ~(build-map-factory rsym r fields)
       ~r)))
