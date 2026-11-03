;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype SeqIter [^:mutable _seq ^:mutable _next]
  Object
  (hasNext [_]
    (if (identical? _seq INIT)
      (do
        (set! _seq START)
        (set! _next (seq _next)))
      (if (identical? _seq _next)
        (set! _next (next _seq))))
    (not (nil? _next)))
  (next [this]
    (if-not ^boolean (.hasNext this)
      (throw (js/Error. "No such element"))
      (do
        (set! _seq _next)
        (first _next))))
  (remove [_] (js/Error. "Unsupported operation")))
