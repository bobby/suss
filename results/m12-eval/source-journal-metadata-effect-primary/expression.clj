(load-string "(defmacro staged-reload \"second\" [] 23)\n[(identical? #'staged-reload #'staged-reload) ((deref (var staged-reload)) nil nil) (:doc (meta (var staged-reload))) (eval '(staged-reload))]\n")
(try (load-string "(defmacro staged-reload \"rejected\" [] 99)\n(defmacro effect-before-rejection [] (alter-meta! #'staged-reload assoc :doc \"effect\") nil)\n(effect-before-rejection)\n(no-such-source-initializer)\n") (catch Throwable _ nil))
(prn (:doc (meta (var staged-reload))))
