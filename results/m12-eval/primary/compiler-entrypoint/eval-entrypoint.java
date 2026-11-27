/**
 *   Copyright (c) Rich Hickey. All rights reserved.
 *   The use and distribution terms for this software are covered by the
 *   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
 *   which can be found in the file epl-v10.html at the root of this distribution.
 *   By using this software in any fashion, you are agreeing to be bound by
 * 	 the terms of this license.
 *   You must not remove this notice, or any other, from this software.
 **/

/* rich Aug 21, 2007 */

public static Object eval(Object form) {
	return eval(form, true);
}

public static Object eval(Object form, boolean freshLoader) {
	boolean createdLoader = false;
	if(true)//!LOADER.isBound())
		{
		Var.pushThreadBindings(RT.map(LOADER, RT.makeClassLoader()));
		createdLoader = true;
		}
	try
		{
		IPersistentMap meta = RT.meta(form);
		Object line = (meta != null ? meta.valAt(RT.LINE_KEY, LINE.deref()) : LINE.deref());
		Object column = (meta != null ? meta.valAt(RT.COLUMN_KEY, COLUMN.deref()) : COLUMN.deref());
		IPersistentMap bindings = RT.mapUniqueKeys(LINE, line, COLUMN, column);
		if(meta != null) {
			Object eval_file = meta.valAt(RT.EVAL_FILE_KEY);
			if(eval_file != null) {
				bindings = bindings.assoc(SOURCE_PATH, eval_file);
				try {
					bindings = bindings.assoc(SOURCE, new File((String)eval_file).getName());
				} catch (Throwable t) {
				}
			}
		}
		Var.pushThreadBindings(bindings);
		try
			{
			form = macroexpand(form);
			if(form instanceof ISeq && Util.equals(RT.first(form), DO))
				{
				ISeq s = RT.next(form);
				for(; RT.next(s) != null; s = RT.next(s))
					eval(RT.first(s), false);
				return eval(RT.first(s), false);
				}
			else if((form instanceof IType) ||
					(form instanceof IPersistentCollection
					&& !(RT.first(form) instanceof Symbol
						&& ((Symbol) RT.first(form)).name.startsWith("def"))))
				{
				ObjExpr fexpr = (ObjExpr) analyze(C.EXPRESSION, RT.list(FN, PersistentVector.EMPTY, form),
													"eval" + RT.nextID());
				IFn fn = (IFn) fexpr.eval();
				return fn.invoke();
				}
			else
				{
				Expr expr = analyze(C.EVAL, form);
				return expr.eval();
				}
			}
		finally
			{
			Var.popThreadBindings();
			}
		}

	finally
		{
		if(createdLoader)
			Var.popThreadBindings();
		}
}

