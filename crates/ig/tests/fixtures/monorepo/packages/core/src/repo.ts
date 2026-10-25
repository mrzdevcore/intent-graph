export class Base {
  save() { return 1; }
}

export class Repo extends Base {
  find(id: string) {
    if (id) { return this.save(); }
    return super.save();
  }
  save() { return 2; }
}

export const handlers = {
  onLoad() { return 1; },
  onSave: () => 2,
};
