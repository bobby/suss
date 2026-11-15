;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- iter-reduce
  ([coll f]
   (let [iter (-iterator coll)]
     (if (.hasNext iter)
       (let [init (.next iter)]
         (loop [acc init]
           (if ^boolean (.hasNext iter)
             (let [nacc (f acc (.next iter))]
               (if (reduced? nacc)
                 @nacc
                 (recur nacc)))
             acc)))
       (f))))
  ([coll f init]
   (let [iter (-iterator coll)]
     (loop [acc init]
       (if ^boolean (.hasNext iter)
         (let [nacc (f acc (.next iter))]
           (if (reduced? nacc)
             @nacc
             (recur nacc)))
         acc)))))
