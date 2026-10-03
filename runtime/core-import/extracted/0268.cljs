;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn array-map
  "keyval => key val
  Returns a new array map with supplied mappings."
  [& keyvals]
  (let [arr (if (and (instance? IndexedSeq keyvals) (zero? (.-i keyvals)))
              (.-arr keyvals)
              (into-array keyvals))]
    (if (odd? (alength arr))
      (throw (js/Error. (str_ "No value supplied for key: " (last arr))))
      (.createAsIfByAssoc PersistentArrayMap arr))))
