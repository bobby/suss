(assert (= "1.12.6" (clojure-version)))
(prn [(do (eval '(defmacro entry-caller-context [] (list 'quote (ns-name *ns*)))) [(eval '(entry-caller-context)) (binding [*ns* (create-ns 'entry.next)] (eval '(user/entry-caller-context)))])
(do (def entry-effects (atom 0)) (eval '(defmacro entry-caller-effect [] (swap! entry-effects inc) 42)) [(eval '(entry-caller-effect)) @entry-effects (eval '(entry-caller-effect)) @entry-effects])])
