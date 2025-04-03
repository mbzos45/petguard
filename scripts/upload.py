import cv2
import requests
from requests.exceptions import RequestException, ConnectionError, HTTPError, Timeout
import time
import sys
import os
import argparse

parser = argparse.ArgumentParser(description="webcam uploader")
parser.add_argument("url", help="upload url")
args = parser.parse_args()

# サーバーのURL
UPLOAD_URL = args.url

# Webカメラを開く
cap = cv2.VideoCapture(0)
if not cap.isOpened():
    print("Failed to open camera")
    exit()

for _ in range(10):
    ret, frame = cap.read()
# 1フレームだけ取得
ret, frame = cap.read()
cap.release()  # すぐにカメラを解放
cv2.destroyAllWindows()
if not ret:
    print("Failed to get frame", file=sys.stderr)
    exit()

# 現在時刻をファイル名にする
timestamp = time.strftime("%Y-%m-%d_%H-%M-%S")
filename = f"{timestamp}.jpg"

# 画像を保存
cv2.imwrite(filename, frame)
print(f"{filename} saved")

# サーバーにアップロード
with open(filename, "rb") as f:
    files = {"file": (filename, f, "image/jpeg")}
    try:
        response = requests.post(UPLOAD_URL, files=files)
    except ConnectionError as ce:
        print("Connection Error:", ce, file=sys.stderr)
        exit()
    except HTTPError as he:
        print("HTTP Error:", he, file=sys.stderr)
        exit()
    except Timeout as te:
        print("Timeout Error:", te, file=sys.stderr)
        exit()
    except RequestException as re:
        print("Error:", re, file=sys.stderr)
        exit()

if response.status_code == 200:
    print("Successes upload :", response.text)
else:
    print("Failed to upload:", response.status_code, response.text, file=sys.stderr)

os.remove(filename)
