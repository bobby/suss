;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn --destructure-map [gmap]
  (if ^boolean LITE_MODE
    (if (implements? ISeq gmap)
      (if (next gmap)
        (.createAsIfByAssoc ObjMap (to-array gmap))
        (if (seq gmap)
          (first gmap)
          (.-EMPTY ObjMap)))
      gmap)
    (if (implements? ISeq gmap)
      (if (next gmap)
        (.createAsIfByAssoc PersistentArrayMap (to-array gmap))
        (if (seq gmap)
          (first gmap)
          (.-EMPTY PersistentArrayMap)))
      gmap)))
