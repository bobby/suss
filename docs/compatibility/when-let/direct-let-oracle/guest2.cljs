
          [(let [x 11] (let [[x y] [x 7] z (+ x y)] [x y z]))
           (loop [n 3 total 0]
             (if (= n 0) total
               (let [[step] [n] {:keys [delta]} {:delta step}]
                 (recur (- n 1) (+ total delta)))))]
        