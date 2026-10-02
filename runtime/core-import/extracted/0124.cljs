;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn name
  "Returns the name String of a string, symbol or keyword."
  [x]
  (if (implements? INamed x)
    (-name x)
    (if (string? x)
      x
      (throw (js/Error. (str_ "Doesn't support name: " x))))))
