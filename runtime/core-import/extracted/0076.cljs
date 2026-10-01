;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn deref
  "Also reader macro: @var/@atom/@delay. Returns the
   most-recently-committed value of ref. When applied to a var
   or atom, returns its current state. When applied to a delay, forces
   it if not already forced. See also - realized?."
  [o]
  (-deref o))
