;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn descendants
  "Returns the immediate and indirect children of tag, through a
  relationship established via derive. h must be a hierarchy obtained
  from make-hierarchy, if not supplied defaults to the global
  hierarchy. Note: does not work on JavaScript type inheritance
  relationships."
  ([tag] (descendants @(get-global-hierarchy) tag))
  ([h tag]
   (if (js-fn? tag)
     (throw (js/Error. "Can't get descendants of constructors"))
     (not-empty (get (:descendants h) tag)))))
