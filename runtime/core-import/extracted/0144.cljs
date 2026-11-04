;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defprotocol IAssociative
  "Protocol for adding associativity to collections."
  (^boolean -contains-key? [coll k]
    "Returns true if k is a key in coll.")
  #_(-entry-at [coll k])
  (^clj -assoc [coll k v]
    "Returns a new collection of coll with a mapping from key k to
     value v added to it."))
