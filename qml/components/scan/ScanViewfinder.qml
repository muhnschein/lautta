// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import QtMultimedia 5.6

Item {
    id: finder

    property bool running
    property bool flashOn
    readonly property bool ready: camera.cameraStatus === Camera.ActiveStatus
    readonly property bool available: camera.availability !== Camera.Unavailable
                                      && camera.availability !== Camera.ResourceMissing

    signal photoSaved(string path)
    signal shotFailed
    signal broken

    function capture(path) {
        camera.imageCapture.captureToLocation(path)
    }

    Camera {
        id: camera

        cameraState: finder.running ? Camera.ActiveState : Camera.UnloadedState
        captureMode: Camera.CaptureStillImage
        metaData.orientation: camera.orientation
        focus.focusMode: Camera.FocusContinuous
        flash.mode: finder.flashOn ? Camera.FlashOn : Camera.FlashOff
        onError: {
            if (errorCode === Camera.CameraError || errorCode === Camera.ServiceMissingError)
                finder.broken()
        }

        imageCapture {
            onImageSaved: finder.photoSaved(decodeURIComponent(path))
            onCaptureFailed: finder.shotFailed()
        }
    }

    VideoOutput {
        anchors.fill: parent
        source: camera
        fillMode: VideoOutput.PreserveAspectFit
    }
}
