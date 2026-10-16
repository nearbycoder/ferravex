// Explicit session overrides never overwrite stored device preferences.
export function configureVisuals(exports,search) {
 const params=new URLSearchParams(search);
 for(const [key,names,apply] of [
  ['look',['authored','warm','cool','ink'],v=>exports.ferravex_look_override(v)],
  ['quality',['low','medium','high'],v=>exports.ferravex_quality_override(v)]
 ]) {
  const value=params.get(key);if(value===null)continue;
  const index=names.indexOf(value);if(index<0)throw new Error(`${key} must be ${names.join(', ')}`);
  apply(index);
 }
}
