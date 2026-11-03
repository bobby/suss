;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- pr-writer
  "Prefer this to pr-seq, because it makes the printing function
   configurable, allowing efficient implementations such as appending
   to a StringBuffer."
  [obj writer opts]
  (if-let [alt-impl (:alt-impl opts)]
    (alt-impl obj writer (-assoc opts :fallback-impl pr-writer-impl))
    (pr-writer-impl obj writer opts)))
