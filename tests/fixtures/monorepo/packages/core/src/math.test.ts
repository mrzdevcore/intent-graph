import { add } from './math';
describe('math', () => {
  it('adds', () => {
    // @intent CORE-1
    expect(add(1, 2)).toBe(3);
  });
  it('adds', () => {});
});
