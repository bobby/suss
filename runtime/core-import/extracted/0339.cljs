;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn update-in
  "'Updates' a value in a nested associative structure, where ks is a
  sequence of keys and f is a function that will take the old value
  and any supplied args and return the new value, and returns a new
  nested structure.  If any levels do not exist, hash-maps will be
  created."
  ([m [k & ks] f]
   (if ks
     (assoc m k (update-in (get m k) ks f))
     (assoc m k (f (get m k)))))
  ([m [k & ks] f a]
   (if ks
     (assoc m k (update-in (get m k) ks f a))
     (assoc m k (f (get m k) a))))
  ([m [k & ks] f a b]
   (if ks
     (assoc m k (update-in (get m k) ks f a b))
     (assoc m k (f (get m k) a b))))
  ([m [k & ks] f a b c]
   (if ks
     (assoc m k (update-in (get m k) ks f a b c))
     (assoc m k (f (get m k) a b c))))
  ([m [k & ks] f a b c & args]
   (if ks
     (assoc m k (apply update-in (get m k) ks f a b c args))
     (assoc m k (apply f (get m k) a b c args)))))
