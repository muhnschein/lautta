// SPDX-License-Identifier: LGPL-2.1-or-later Camera and its live picture, for a portrait page.
// Camera device held only while `running`.
import QtQuick 2.6
import QtMultimedia 5.6

Item {
    id: finder

    property bool running
    property bool flashOn
    readonly property bool ready: camera.cameraStatus === Camera.ActiveStatus
    // False: no camera here. Busy counts as there.
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

        // Qt starts a camera once built: unloaded unless wanted.
        cameraState: finder.running ? Camera.ActiveState : Camera.UnloadedState
        captureMode: Camera.CaptureStillImage
        // File's turn = sensor's mounting: page never turns with the phone.
        metaData.orientation: camera.orientation
        focus.focusMode: Camera.FocusContinuous
        flash.mode: finder.flashOn ? Camera.FlashOn : Camera.FlashOff
        onError: {
            if (errorCode === Camera.CameraError || errorCode === Camera.ServiceMissingError)
                finder.broken()
        }

        imageCapture {
            // Path comes percent-encoded.
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
