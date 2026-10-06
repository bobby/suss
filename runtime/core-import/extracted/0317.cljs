;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn print-prefix-map [prefix m print-one writer opts]
  (pr-sequential-writer
    writer
    (fn [e w opts]
      (do (print-one (key e) w opts)
          (-write w \space)
          (print-one (val e) w opts)))
    (str_ prefix "{") ", " "}"
    opts (seq m)))
