(println (pr-str (try ((identity char) (float -70000.1)) (catch Throwable e (.getMessage e)))))
(println (pr-str (try ((identity char) -70000.1015625) (catch Throwable e (.getMessage e)))))
(println (pr-str (try (char (float -70000.1)) (catch Throwable e (.getMessage e)))))
