import { util, double } from '@acme/core';
export default function Button(props: { n: number }) {
  return <button>{util.upper(String(double(props.n)))}</button>;
}
