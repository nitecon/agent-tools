package service

type Service interface {
	Run()
}

type Worker struct{}

func (Worker) Run() {}

func Dispatch(service Service) {
	service.Run()
}
