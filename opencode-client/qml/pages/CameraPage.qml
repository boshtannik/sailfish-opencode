import QtQuick 2.6
import QtMultimedia 5.4
import Sailfish.Silica 1.0

// Take a photo (or record a video, for models that read video) and attach
// it to the next prompt. The capture lands in Pictures/Camera or
// Videos/Camera like the system camera's, so it also shows in Gallery.
// The camera settings mirror the Jolla camera app (gst-droid).
Page {
    id: cameraPage

    property var appWindow
    // Video mode is offered only when the current model reads video.
    property bool allowVideo: false
    property bool videoMode: false
    readonly property bool recording: camera.videoRecorder.recorderState === CameraRecorder.RecordingState
    property string error: ""
    // Seconds recorded so far (shown while recording).
    property int recordSeconds: 0

    allowedOrientations: Orientation.Portrait
    backNavigation: !recording

    function stamp() {
        var d = new Date()
        function p(n) { return (n < 10 ? "0" : "") + n }
        return d.getFullYear() + p(d.getMonth() + 1) + p(d.getDate()) + "_"
               + p(d.getHours()) + p(d.getMinutes()) + p(d.getSeconds())
    }

    function shutter() {
        cameraPage.error = ""
        if (cameraPage.videoMode) {
            if (cameraPage.recording) {
                camera.videoRecorder.stop()
            } else {
                camera.videoRecorder.outputLocation = StandardPaths.videos + "/Camera/opencode_"
                                                      + cameraPage.stamp() + ".mp4"
                cameraPage.recordSeconds = 0
                camera.videoRecorder.record()
            }
        } else if (camera.imageCapture.ready) {
            camera.imageCapture.captureToLocation(StandardPaths.pictures + "/Camera/opencode_"
                                                  + cameraPage.stamp() + ".jpg")
        }
    }

    // Done: attach the file and go back to the chat.
    function finish(path) {
        if (path && ("" + path) !== "")
            cameraPage.appWindow.addAttachment("" + path)
        pageStack.pop()
    }

    Rectangle {
        anchors.fill: parent
        color: "black"
    }

    Camera {
        id: camera
        captureMode: cameraPage.videoMode ? Camera.CaptureVideo : Camera.CaptureStillImage
        // Only hold the camera while this page is on screen.
        cameraState: cameraPage.status === PageStatus.Active || cameraPage.recording
                     ? Camera.ActiveState : Camera.UnloadedState
        metaData.orientation: camera.orientation

        imageCapture {
            onImageSaved: cameraPage.finish(path)
            onCaptureFailed: {
                console.warn("CameraPage: photo capture failed: " + message)
                cameraPage.error = message
            }
        }
        videoRecorder {
            audioChannels: 2
            audioSampleRate: 48000
            audioCodec: "audio/mpeg, mpegversion=(int)4"
            videoCodec: "video/x-h264"
            mediaContainer: "video/quicktime, variant=(string)iso"
            onRecorderStateChanged: {
                if (camera.videoRecorder.recorderState === CameraRecorder.StoppedState
                        && camera.videoRecorder.actualLocation.toString() !== "")
                    cameraPage.finish(camera.videoRecorder.actualLocation.toString()
                                      .replace("file://", ""))
            }
        }
        onError: {
            console.warn("CameraPage: camera error " + errorCode + ": " + errorString)
            cameraPage.error = errorString
        }
    }

    VideoOutput {
        id: viewfinder
        anchors.fill: parent
        source: camera
        fillMode: VideoOutput.PreserveAspectFit
        orientation: camera.orientation
    }

    Timer {
        interval: 1000
        repeat: true
        running: cameraPage.recording
        onTriggered: cameraPage.recordSeconds++
    }

    // ── top: recording time / error ──
    Label {
        anchors {
            top: parent.top
            topMargin: Theme.paddingLarge * 2
            horizontalCenter: parent.horizontalCenter
        }
        visible: cameraPage.recording || cameraPage.error !== ""
        text: cameraPage.error !== "" ? cameraPage.error
              : ("● " + Math.floor(cameraPage.recordSeconds / 60) + ":"
                 + ((cameraPage.recordSeconds % 60) < 10 ? "0" : "") + (cameraPage.recordSeconds % 60))
        color: cameraPage.error !== "" ? Theme.errorColor : "#ff4040"
        font.pixelSize: Theme.fontSizeMedium
    }

    // ── bottom: photo/video switch, shutter, front/back camera ──
    Rectangle {
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        height: controls.height + 2 * Theme.paddingLarge
        color: Theme.rgba("black", 0.45)

        Item {
            id: controls
            anchors.centerIn: parent
            width: parent.width
            height: Theme.itemSizeHuge

            // Photo / Video (only when the model reads video).
            Column {
                visible: cameraPage.allowVideo
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                spacing: Theme.paddingSmall
                Repeater {
                    model: [false, true]
                    Label {
                        text: modelData ? qsTr("Video") : qsTr("Photo")
                        color: cameraPage.videoMode === modelData ? Theme.highlightColor : "white"
                        font.pixelSize: Theme.fontSizeSmall
                        MouseArea {
                            anchors.fill: parent
                            anchors.margins: -Theme.paddingMedium
                            enabled: !cameraPage.recording
                            onClicked: cameraPage.videoMode = modelData
                        }
                    }
                }
            }

            // Shutter: white ring; red dot for video, red square while recording.
            MouseArea {
                id: shutterArea
                anchors.centerIn: parent
                width: Theme.itemSizeLarge
                height: width
                enabled: camera.cameraStatus === Camera.ActiveStatus
                onClicked: cameraPage.shutter()
                Rectangle {
                    anchors.fill: parent
                    radius: width / 2
                    color: "transparent"
                    border.width: Math.round(Theme.paddingSmall * 1.2)
                    border.color: shutterArea.enabled ? "white" : Theme.rgba("white", 0.4)
                }
                Rectangle {
                    anchors.centerIn: parent
                    width: cameraPage.recording ? parent.width * 0.36 : parent.width * 0.72
                    height: width
                    radius: cameraPage.recording ? Theme.paddingSmall : width / 2
                    color: cameraPage.videoMode ? "#e53935"
                                                : (shutterArea.pressed ? Theme.highlightColor : "white")
                    Behavior on width { NumberAnimation { duration: 120 } }
                }
            }

            IconButton {
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                visible: QtMultimedia.availableCameras.length > 1
                enabled: !cameraPage.recording
                icon.source: camera.position === Camera.FrontFace
                             ? "image://theme/icon-camera-backcamera?" + "white"
                             : "image://theme/icon-camera-front-camera?" + "white"
                onClicked: {
                    var want = camera.position === Camera.FrontFace ? Camera.BackFace : Camera.FrontFace
                    var cams = QtMultimedia.availableCameras
                    for (var i = 0; i < cams.length; i++) {
                        if (cams[i].position === want) {
                            camera.deviceId = cams[i].deviceId
                            return
                        }
                    }
                }
            }
        }
    }
}
