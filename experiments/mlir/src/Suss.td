// Original Suss spike code, licensed under the repository MIT/Apache-2.0 terms.
include "mlir/IR/OpBase.td"
include "mlir/IR/AttrTypeBase.td"
def Suss_Dialect : Dialect {
  let name = "suss";
  let cppNamespace = "::suss";
  let useDefaultTypePrinterParser = 1;
  let hasOperationAttrVerify = 1;
}
def Suss_Closure : TypeDef<Suss_Dialect, "Closure"> {
  let mnemonic = "closure";
  let genVerifyDecl = 1;
  let parameters = (ins "::mlir::FunctionType":$signature,
      ArrayRefParameter<"::mlir::Type">:$captures);
  let assemblyFormat = "`<` $signature `,` custom<CaptureTypes>($captures) `>`";
}
class Suss_Op<string name, list<Trait> traits = []> : Op<Suss_Dialect, name, traits>;
def Suss_ConstOp : Suss_Op<"const"> {
  let arguments = (ins F64Attr:$value);
  let results = (outs F64:$result);
}
def Suss_AddOp : Suss_Op<"add"> {
  let arguments = (ins F64:$lhs, F64:$rhs);
  let results = (outs F64:$result);
}
def Suss_ClosureOp : Suss_Op<"closure", [SingleBlock]> {
  let arguments = (ins Variadic<AnyType>:$captures);
  let results = (outs AnyType:$result);
  let regions = (region AnyRegion:$body);
  let hasVerifier = 1;
}
def Suss_CallOp : Suss_Op<"call"> {
  let arguments = (ins AnyType:$callee, Variadic<AnyType>:$arguments);
  let results = (outs AnyType:$result);
  let hasVerifier = 1;
}
def Suss_ReturnOp : Suss_Op<"return", [Terminator]> {
  let arguments = (ins AnyType:$value);
  let hasVerifier = 1;
}

def Suss_Value : TypeDef<Suss_Dialect, "Value"> { let mnemonic = "value"; }
def Suss_EffectClosure : TypeDef<Suss_Dialect, "EffectClosure"> {
 let mnemonic = "effect_closure"; let genVerifyDecl = 1;
 let parameters = (ins "::mlir::FunctionType":$signature, ArrayRefParameter<"::mlir::Type">:$captures);
 let assemblyFormat = "`<` $signature `,` custom<CaptureTypes>($captures) `>`";
}
def Suss_BoolOp : Suss_Op<"bool"> { let arguments = (ins BoolAttr:$value); let results = (outs I1:$result); }
def Suss_EffectClosureOp : Suss_Op<"effect_closure", [SingleBlock]> {
 let arguments = (ins Variadic<AnyType>:$captures); let results = (outs AnyType:$result);
 let regions = (region AnyRegion:$body); let hasVerifier = 1;
}
def Suss_EffectCallOp : Suss_Op<"effect_call"> {
 let arguments = (ins AnyType:$callee, Variadic<AnyType>:$arguments); let results = (outs AnyType:$result); let hasVerifier = 1;
}
def Suss_ReadOp : Suss_Op<"read"> { let arguments = (ins StrAttr:$namespace_name, StrAttr:$name); let results = (outs AnyType:$result); let hasVerifier = 1; }
def Suss_WriteOp : Suss_Op<"write"> { let arguments = (ins StrAttr:$namespace_name, StrAttr:$name, AnyType:$value); let results = (outs AnyType:$result); let hasVerifier = 1; }
def Suss_DynamicArithmeticOp : Suss_Op<"dynamic_arithmetic"> { let arguments = (ins StrAttr:$kind, AnyType:$lhs, AnyType:$rhs); let results = (outs AnyType:$result); let hasVerifier = 1; }
def Suss_EqualOp : Suss_Op<"equal"> { let arguments = (ins AnyType:$lhs, AnyType:$rhs); let results = (outs I1:$result); let hasVerifier = 1; }
def Suss_IfOp : Suss_Op<"if"> { let arguments = (ins I1:$condition); let results = (outs AnyType:$result); let regions = (region AnyRegion:$consequent, AnyRegion:$alternative); let hasVerifier = 1; }
def Suss_TryOp : Suss_Op<"try"> { let arguments = (ins AnyType:$body, AnyType:$handler, AnyType:$cleanup); let results = (outs AnyType:$result); let hasVerifier = 1; }
def Suss_EffectReturnOp : Suss_Op<"effect_return", [Terminator]> { let arguments = (ins AnyType:$value); let hasVerifier = 1; }
def Suss_YieldOp : Suss_Op<"yield", [Terminator]> { let arguments = (ins AnyType:$value); let hasVerifier = 1; }
def Suss_ThrowOp : Suss_Op<"throw", [Terminator]> { let arguments = (ins F64:$value); let hasVerifier = 1; }
