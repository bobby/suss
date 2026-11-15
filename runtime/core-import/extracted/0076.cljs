;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn reduce
  "f should be a function of 2 arguments. If val is not supplied,
  returns the result of applying f to the first 2 items in coll, then
  applying f to that result and the 3rd item, etc. If coll contains no
  items, f must accept no arguments as well, and reduce returns the
  result of calling f with no arguments.  If coll has only 1 item, it
  is returned and f is not called.  If val is supplied, returns the
  result of applying f to val and the first item in coll, then
  applying f to that result and the 2nd item, etc. If coll contains no
  items, returns val and f is not called."
  ([f coll]
     (cond
       (implements? IReduce coll)
       (-reduce coll f)

       (array? coll)
       (array-reduce coll f)

       (string? coll)
       (array-reduce coll f)

       (native-satisfies? IReduce coll)
       (-reduce coll f)

       (iterable? coll)
       (iter-reduce coll f)

       :else
       (seq-reduce f coll)))
  ([f val coll]
     (cond
       (implements? IReduce coll)
       (-reduce coll f val)

       (array? coll)
       (array-reduce coll f val)

       (string? coll)
       (array-reduce coll f val)

       (native-satisfies? IReduce coll)
       (-reduce coll f val)

       (iterable? coll)
       (iter-reduce coll f val)

       :else
       (seq-reduce f val coll))))
