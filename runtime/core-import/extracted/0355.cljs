;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defprotocol ISorted
  "Protocol for a collection which can represent their items
  in a sorted manner. "
  (^clj -sorted-seq [coll ascending?]
    "Returns a sorted seq from coll in either ascending or descending order.")
  (^clj -sorted-seq-from [coll k ascending?]
    "Returns a sorted seq from coll in either ascending or descending order.
     If ascending is true, the result should contain all items which are > or >=
     than k. If ascending is false, the result should contain all items which
     are < or <= than k, e.g.
     (-sorted-seq-from (sorted-set 1 2 3 4 5) 3 true) => (3 4 5)
     (-sorted-seq-from (sorted-set 1 2 3 4 5) 3 false) => (3 2 1)")
  (-entry-key [coll entry]
    "Returns the key for entry.")
  (-comparator [coll]
    "Returns the comparator for coll."))
