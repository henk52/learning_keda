#!/usr/bin/env bash

# Deploy all the parts required for a keda test environment.

kubectl apply -f deployment/namespace.yaml

kubectl apply -f deployment/otel-lgtm.yaml

kubectl apply -f deployment/api_service_emulator_minikube.yaml

# TODO wait until the otel-lgtm deployment is ready before proceeding.
kubectl apply -f deployment/k6.yaml
