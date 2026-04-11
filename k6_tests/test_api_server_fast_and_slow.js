import http from 'k6/http';
import { sleep } from 'k6';

export const options = {
  vus: 40,
  duration: '30s',
};

export default function () {
  // 67% fast, 33% slow
  if (Math.random() < 0.67) {
    http.get('http://api-service:8080/fast');
  } else {
    http.get('http://api-service:8080/slow');
  }
}