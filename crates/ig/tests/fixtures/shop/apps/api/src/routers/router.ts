export class Router {
  post(path: string, handler: (body: any) => unknown) {
    return { path, handler };
  }
}
