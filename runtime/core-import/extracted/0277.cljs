;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn hash-map
  "keyval => key val
  Returns a new hash map with supplied mappings."
  [& keyvals]
  (loop [in (seq keyvals), out (transient (.-EMPTY PersistentHashMap))]
    (if in
      (let [in' (next in)]
        (if (nil? in')
          (throw (js/Error. (str_ "No value supplied for key: " (first in))))
          (recur (next in') (assoc! out (first in) (first in')) )))
      (persistent! out))))
