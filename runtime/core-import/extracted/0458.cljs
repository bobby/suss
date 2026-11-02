;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype TaggedLiteral [tag form]
  Object
  (toString [coll]
    (pr-str* coll))

  IEquiv
  (-equiv [this other]
    (and (instance? TaggedLiteral other)
         (= tag (.-tag other))
         (= form (.-form other))))

  IHash
  (-hash [this]
    (+ (* 31 (hash tag))
       (hash form)))

  ILookup
  (-lookup [this v]
    (-lookup this v nil))
  (-lookup [this v not-found]
    (case v
      :tag tag
      :form form
      not-found))

  IPrintWithWriter
  (-pr-writer [o writer opts]
    (-write writer (str_ "#" tag " "))
    (pr-writer form writer opts)))
