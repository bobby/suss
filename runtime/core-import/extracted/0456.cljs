;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn partition
  "Returns a lazy sequence of lists of n items each, at offsets step
  apart. If step is not supplied, defaults to n, i.e. the partitions
  do not overlap. If a pad collection is supplied, use its elements as
  necessary to complete last partition up to n items. In case there are
  not enough padding elements, return a partition with less than n items."
  ([n coll]
     (partition n n coll))
  ([n step coll]
     (lazy-seq
       (when-let [s (seq coll)]
         (let [p (take n s)]
           (when (== n (count p))
             (cons p (partition n step (drop step s))))))))
  ([n step pad coll]
     (lazy-seq
       (when-let [s (seq coll)]
         (let [p (take n s)]
           (if (== n (count p))
             (cons p (partition n step pad (drop step s)))
             (list (take n (concat p pad)))))))))
