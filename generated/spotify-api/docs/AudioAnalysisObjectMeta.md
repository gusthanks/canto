# AudioAnalysisObjectMeta

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**analyzer_version** | Option<**String**> | The version of the Analyzer used to analyze this track. | [optional]
**platform** | Option<**String**> | The platform used to read the track's audio data. | [optional]
**detailed_status** | Option<**String**> | A detailed status code for this track. If analysis data is missing, this code may explain why. | [optional]
**status_code** | Option<**i32**> | The return code of the analyzer process. 0 if successful, 1 if any errors occurred. | [optional]
**timestamp** | Option<**i32**> | The Unix timestamp (in seconds) at which this track was analyzed. | [optional]
**analysis_time** | Option<**f64**> | The amount of time taken to analyze this track. | [optional]
**input_process** | Option<**String**> | The method used to read the track's audio data. | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


