;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype Symbol [ns name str ^:mutable _hash _meta]
  Object
  (toString [_] str)
  (equiv [this other] (-equiv this other))

  IEquiv
  (-equiv [_ other]
    (if (instance? Symbol other)
      (identical? str (.-str other))
      false))

  IFn
  (-invoke [sym coll]
    (get coll sym))
  (-invoke [sym coll not-found]
    (get coll sym not-found))

  IMeta
  (-meta [_] _meta)

  IWithMeta
  (-with-meta [_ new-meta] (Symbol. ns name str _hash new-meta))

  IHash
  (-hash [sym]
    (caching-hash sym hash-symbol _hash))

  INamed
  (-name [_] name)
  (-namespace [_] ns)

  IPrintWithWriter
  (-pr-writer [o writer _] (-write writer str)))
