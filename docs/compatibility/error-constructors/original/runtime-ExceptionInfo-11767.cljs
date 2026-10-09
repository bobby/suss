(defn ^{:jsdoc ["@constructor"]}
  ExceptionInfo [message data cause]
  (let [e (js/Error. message)]
    (this-as this
      (set! (.-message this) message)
      (set! (.-data this) data)
      (set! (.-cause this) cause)
      (do
        (set! (.-name this) (.-name e))
        ;; non-standard
        (set! (.-description this) (.-description e))
        (set! (.-number this) (.-number e))
        (set! (.-fileName this) (.-fileName e))
        (set! (.-lineNumber this) (.-lineNumber e))
        (set! (.-columnNumber this) (.-columnNumber e))
        (set! (.-stack this) (.-stack e)))
      this)))
