;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defprotocol IStack
  "Protocol for collections to provide access to their items as stacks. The top
  of the stack should be accessed in the most efficient way for the different
  data structures."
  (-peek [coll]
    "Returns the item from the top of the stack. Is used by cljs.core/peek.")
  (^clj -pop [coll]
    "Returns a new stack without the item on top of the stack. Is used
     by cljs.core/pop."))
