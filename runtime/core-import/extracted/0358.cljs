;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn vary-meta
 "Returns an object of the same type and value as obj, with
  (apply f (meta obj) args) as its metadata."
  ([obj f]
   (with-meta obj (f (meta obj))))
  ([obj f a]
   (with-meta obj (f (meta obj) a)))
  ([obj f a b]
   (with-meta obj (f (meta obj) a b)))
  ([obj f a b c]
   (with-meta obj (f (meta obj) a b c)))
  ([obj f a b c d]
   (with-meta obj (f (meta obj) a b c d)))
  ([obj f a b c d & args]
   (with-meta obj (apply f (meta obj) a b c d args))))
