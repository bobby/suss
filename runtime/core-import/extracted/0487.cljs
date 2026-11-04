;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype Delay [^:mutable f ^:mutable value]
  IDeref
  (-deref [_]
    (when f
      (set! value (f))
      (set! f nil))
    value)

  IPending
  (-realized? [x]
    (not f))

  IPrintWithWriter
  (-pr-writer [x writer opts]
    (-write writer "#object[cljs.core.Delay ")
    (pr-writer {:status (if (nil? f) :ready :pending), :val value} writer opts)
    (-write writer "]")))
