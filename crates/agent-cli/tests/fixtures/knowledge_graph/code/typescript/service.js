export class Worker {
  run() {}
}

export function dispatch(service) {
  service.run();
}
