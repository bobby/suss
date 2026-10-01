;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defprotocol ISeq
  "Protocol for collections to provide access to their items as sequences."
  (-first [coll]
    "Returns the first item in the collection coll. Used by cljs.core/first.")
  (^clj -rest [coll]
    "Returns a new collection of coll without the first item. It should
     always return a seq, e.g.
     (rest []) => ()
     (rest nil) => ()"))
