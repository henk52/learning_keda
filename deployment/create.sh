#!/usr/bin/env bash

# Deploy all the parts required for a keda test environment.

kubectl apply -f deployment/namespace.yaml

kubectl apply -f deployment/otel-lgtm.yaml

kubectl apply -f deployment/api_service_emulator_minikube.yaml

echo "III Waiting for otel-lgtm deployment to be ready"
kubectl wait --for=condition=available --timeout=360s deployment/otel-lgtm -n testing-keda

# TODO wait until the otel-lgtm deployment is ready before proceeding.
kubectl apply -f deployment/k6.yaml

echo "III to access the grafana from your host:"
echo "kubectl port-forward -n testing-keda svc/otel-lgtm 3000:3000"
