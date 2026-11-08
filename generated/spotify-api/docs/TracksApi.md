# \TracksApi

All URIs are relative to *https://api.spotify.com/v1*

Method | HTTP request | Description
------------- | ------------- | -------------
[**add_items_to_playlist**](TracksApi.md#add_items_to_playlist) | **POST** /playlists/{playlist_id}/items | Add Items to Playlist 
[**add_tracks_to_playlist**](TracksApi.md#add_tracks_to_playlist) | **POST** /playlists/{playlist_id}/tracks | Add Items to Playlist [DEPRECATED] 
[**check_users_saved_tracks**](TracksApi.md#check_users_saved_tracks) | **GET** /me/tracks/contains | Check User's Saved Tracks 
[**get_an_albums_tracks**](TracksApi.md#get_an_albums_tracks) | **GET** /albums/{id}/tracks | Get Album Tracks 
[**get_an_artists_top_tracks**](TracksApi.md#get_an_artists_top_tracks) | **GET** /artists/{id}/top-tracks | Get Artist's Top Tracks 
[**get_audio_analysis**](TracksApi.md#get_audio_analysis) | **GET** /audio-analysis/{id} | Get Track's Audio Analysis 
[**get_audio_features**](TracksApi.md#get_audio_features) | **GET** /audio-features/{id} | Get Track's Audio Features 
[**get_playlists_items**](TracksApi.md#get_playlists_items) | **GET** /playlists/{playlist_id}/items | Get Playlist Items 
[**get_playlists_tracks**](TracksApi.md#get_playlists_tracks) | **GET** /playlists/{playlist_id}/tracks | Get Playlist Items [DEPRECATED] 
[**get_recommendations**](TracksApi.md#get_recommendations) | **GET** /recommendations | Get Recommendations 
[**get_several_audio_features**](TracksApi.md#get_several_audio_features) | **GET** /audio-features | Get Several Tracks' Audio Features 
[**get_several_tracks**](TracksApi.md#get_several_tracks) | **GET** /tracks | Get Several Tracks 
[**get_track**](TracksApi.md#get_track) | **GET** /tracks/{id} | Get Track 
[**get_users_saved_tracks**](TracksApi.md#get_users_saved_tracks) | **GET** /me/tracks | Get User's Saved Tracks 
[**get_users_top_artists_and_tracks**](TracksApi.md#get_users_top_artists_and_tracks) | **GET** /me/top/{type} | Get User's Top Items 
[**remove_items_playlist**](TracksApi.md#remove_items_playlist) | **DELETE** /playlists/{playlist_id}/items | Remove Playlist Items 
[**remove_tracks_playlist**](TracksApi.md#remove_tracks_playlist) | **DELETE** /playlists/{playlist_id}/tracks | Remove Playlist Items [DEPRECATED] 
[**remove_tracks_user**](TracksApi.md#remove_tracks_user) | **DELETE** /me/tracks | Remove User's Saved Tracks 
[**reorder_or_replace_playlists_items**](TracksApi.md#reorder_or_replace_playlists_items) | **PUT** /playlists/{playlist_id}/items | Update Playlist Items 
[**reorder_or_replace_playlists_tracks**](TracksApi.md#reorder_or_replace_playlists_tracks) | **PUT** /playlists/{playlist_id}/tracks | Update Playlist Items [DEPRECATED] 
[**save_tracks_user**](TracksApi.md#save_tracks_user) | **PUT** /me/tracks | Save Tracks for Current User 



## add_items_to_playlist

> models::ReorderOrReplacePlaylistsTracks200Response add_items_to_playlist(playlist_id, position, uris, add_tracks_to_playlist_request)
Add Items to Playlist 

Add one or more items to a user's playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**position** | Option<**i32**> |  |  |
**uris** | Option<**String**> |  |  |
**add_tracks_to_playlist_request** | Option<[**AddTracksToPlaylistRequest**](AddTracksToPlaylistRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## add_tracks_to_playlist

> models::ReorderOrReplacePlaylistsTracks200Response add_tracks_to_playlist(playlist_id, position, uris, add_tracks_to_playlist_request)
Add Items to Playlist [DEPRECATED] 

**Deprecated:** Use [Add Items to Playlist](/documentation/web-api/reference/add-items-to-playlist) instead.  Add one or more items to a user's playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**position** | Option<**i32**> |  |  |
**uris** | Option<**String**> |  |  |
**add_tracks_to_playlist_request** | Option<[**AddTracksToPlaylistRequest**](AddTracksToPlaylistRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## check_users_saved_tracks

> Vec<bool> check_users_saved_tracks(ids)
Check User's Saved Tracks 

Check if one or more tracks is already saved in the current Spotify user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_an_albums_tracks

> models::PagingSimplifiedTrackObject get_an_albums_tracks(id, market, limit, offset)
Get Album Tracks 

Get Spotify catalog information about an album’s tracks. Optional parameters can be used to limit the number of tracks returned. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingSimplifiedTrackObject**](PagingSimplifiedTrackObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_an_artists_top_tracks

> models::GetAnArtistsTopTracks200Response get_an_artists_top_tracks(id, market)
Get Artist's Top Tracks 

Get Spotify catalog information about an artist's top tracks by country. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

[**models::GetAnArtistsTopTracks200Response**](get_an_artists_top_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_audio_analysis

> models::AudioAnalysisObject get_audio_analysis(id)
Get Track's Audio Analysis 

Get a low-level audio analysis for a track in the Spotify catalog. The audio analysis describes the track’s structure and musical content, including rhythm, pitch, and timbre. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |

### Return type

[**models::AudioAnalysisObject**](AudioAnalysisObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_audio_features

> models::AudioFeaturesObject get_audio_features(id)
Get Track's Audio Features 

Get audio feature information for a single track identified by its unique Spotify ID. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |

### Return type

[**models::AudioFeaturesObject**](AudioFeaturesObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_playlists_items

> models::PagingPlaylistTrackObject get_playlists_items(playlist_id, market, fields, limit, offset, additional_types)
Get Playlist Items 

Get full details of the items of a playlist owned by a Spotify user.  **Note**: This endpoint is only accessible for playlists owned by the current user or playlists the user is a collaborator of. A `403 Forbidden` status code will be returned if the user is neither the owner nor a collaborator of the playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |
**fields** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]
**additional_types** | Option<**String**> |  |  |

### Return type

[**models::PagingPlaylistTrackObject**](PagingPlaylistTrackObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_playlists_tracks

> models::PagingPlaylistTrackObject get_playlists_tracks(playlist_id, market, fields, limit, offset, additional_types)
Get Playlist Items [DEPRECATED] 

**Deprecated:** Use [Get Playlist Items](/documentation/web-api/reference/get-playlists-items) instead.  Get full details of the items of a playlist owned by a Spotify user. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |
**fields** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]
**additional_types** | Option<**String**> |  |  |

### Return type

[**models::PagingPlaylistTrackObject**](PagingPlaylistTrackObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_recommendations

> models::RecommendationsObject get_recommendations(seed_artists, seed_genres, seed_tracks, limit, market, min_acousticness, max_acousticness, target_acousticness, min_danceability, max_danceability, target_danceability, min_duration_ms, max_duration_ms, target_duration_ms, min_energy, max_energy, target_energy, min_instrumentalness, max_instrumentalness, target_instrumentalness, min_key, max_key, target_key, min_liveness, max_liveness, target_liveness, min_loudness, max_loudness, target_loudness, min_mode, max_mode, target_mode, min_popularity, max_popularity, target_popularity, min_speechiness, max_speechiness, target_speechiness, min_tempo, max_tempo, target_tempo, min_time_signature, max_time_signature, target_time_signature, min_valence, max_valence, target_valence)
Get Recommendations 

Recommendations are generated based on the available information for a given seed entity and matched against similar artists and tracks. If there is sufficient information about the provided seeds, a list of tracks will be returned together with pool size details.  For artists and tracks that are very new or obscure there might not be enough data to generate a list of tracks. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**seed_artists** | **String** |  | [required] |
**seed_genres** | **String** |  | [required] |
**seed_tracks** | **String** |  | [required] |
**limit** | Option<**i32**> |  |  |[default to 20]
**market** | Option<**String**> |  |  |
**min_acousticness** | Option<**f64**> |  |  |
**max_acousticness** | Option<**f64**> |  |  |
**target_acousticness** | Option<**f64**> |  |  |
**min_danceability** | Option<**f64**> |  |  |
**max_danceability** | Option<**f64**> |  |  |
**target_danceability** | Option<**f64**> |  |  |
**min_duration_ms** | Option<**i32**> |  |  |
**max_duration_ms** | Option<**i32**> |  |  |
**target_duration_ms** | Option<**i32**> |  |  |
**min_energy** | Option<**f64**> |  |  |
**max_energy** | Option<**f64**> |  |  |
**target_energy** | Option<**f64**> |  |  |
**min_instrumentalness** | Option<**f64**> |  |  |
**max_instrumentalness** | Option<**f64**> |  |  |
**target_instrumentalness** | Option<**f64**> |  |  |
**min_key** | Option<**i32**> |  |  |
**max_key** | Option<**i32**> |  |  |
**target_key** | Option<**i32**> |  |  |
**min_liveness** | Option<**f64**> |  |  |
**max_liveness** | Option<**f64**> |  |  |
**target_liveness** | Option<**f64**> |  |  |
**min_loudness** | Option<**f64**> |  |  |
**max_loudness** | Option<**f64**> |  |  |
**target_loudness** | Option<**f64**> |  |  |
**min_mode** | Option<**i32**> |  |  |
**max_mode** | Option<**i32**> |  |  |
**target_mode** | Option<**i32**> |  |  |
**min_popularity** | Option<**i32**> |  |  |
**max_popularity** | Option<**i32**> |  |  |
**target_popularity** | Option<**i32**> |  |  |
**min_speechiness** | Option<**f64**> |  |  |
**max_speechiness** | Option<**f64**> |  |  |
**target_speechiness** | Option<**f64**> |  |  |
**min_tempo** | Option<**f64**> |  |  |
**max_tempo** | Option<**f64**> |  |  |
**target_tempo** | Option<**f64**> |  |  |
**min_time_signature** | Option<**i32**> |  |  |
**max_time_signature** | Option<**i32**> |  |  |
**target_time_signature** | Option<**i32**> |  |  |
**min_valence** | Option<**f64**> |  |  |
**max_valence** | Option<**f64**> |  |  |
**target_valence** | Option<**f64**> |  |  |

### Return type

[**models::RecommendationsObject**](RecommendationsObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_several_audio_features

> models::GetSeveralAudioFeatures200Response get_several_audio_features(ids)
Get Several Tracks' Audio Features 

Get audio features for multiple tracks based on their Spotify IDs. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

[**models::GetSeveralAudioFeatures200Response**](get_several_audio_features_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_several_tracks

> models::GetAnArtistsTopTracks200Response get_several_tracks(ids, market)
Get Several Tracks 

Get Spotify catalog information for multiple tracks based on their Spotify IDs. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

[**models::GetAnArtistsTopTracks200Response**](get_an_artists_top_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_track

> models::TrackObject get_track(id, market)
Get Track 

Get Spotify catalog information for a single track identified by its unique Spotify ID. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

[**models::TrackObject**](TrackObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_saved_tracks

> models::PagingSavedTrackObject get_users_saved_tracks(market, limit, offset)
Get User's Saved Tracks 

Get a list of the songs saved in the current Spotify user's 'Your Music' library. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**market** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingSavedTrackObject**](PagingSavedTrackObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_top_artists_and_tracks

> models::GetUsersTopArtistsAndTracks200Response get_users_top_artists_and_tracks(r#type, time_range, limit, offset)
Get User's Top Items 

Get the current user's top artists or tracks based on calculated affinity. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**time_range** | Option<**String**> |  |  |[default to medium_term]
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::GetUsersTopArtistsAndTracks200Response**](get_users_top_artists_and_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_items_playlist

> models::ReorderOrReplacePlaylistsTracks200Response remove_items_playlist(playlist_id, remove_items_playlist_request)
Remove Playlist Items 

Remove one or more items from a user's playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**remove_items_playlist_request** | Option<[**RemoveItemsPlaylistRequest**](RemoveItemsPlaylistRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_tracks_playlist

> models::ReorderOrReplacePlaylistsTracks200Response remove_tracks_playlist(playlist_id, remove_tracks_playlist_request)
Remove Playlist Items [DEPRECATED] 

**Deprecated:** Use [Remove Playlist Items](/documentation/web-api/reference/remove-items-playlist) instead.  Remove one or more items from a user's playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**remove_tracks_playlist_request** | Option<[**RemoveTracksPlaylistRequest**](RemoveTracksPlaylistRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_tracks_user

> remove_tracks_user(ids, save_albums_user_request)
Remove User's Saved Tracks 

Remove one or more tracks from the current user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**save_albums_user_request** | Option<[**SaveAlbumsUserRequest**](SaveAlbumsUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## reorder_or_replace_playlists_items

> models::ReorderOrReplacePlaylistsTracks200Response reorder_or_replace_playlists_items(playlist_id, uris, reorder_or_replace_playlists_tracks_request)
Update Playlist Items 

Either reorder or replace items in a playlist depending on the request's parameters. To reorder items, include `range_start`, `insert_before`, `range_length` and `snapshot_id` in the request's body. To replace items, include `uris` as either a query parameter or in the request's body. Replacing items in a playlist will overwrite its existing items. This operation can be used for replacing or clearing items in a playlist. <br/> **Note**: Replace and reorder are mutually exclusive operations which share the same endpoint, but have different parameters. These operations can't be applied together in a single request. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**uris** | Option<**String**> |  |  |
**reorder_or_replace_playlists_tracks_request** | Option<[**ReorderOrReplacePlaylistsTracksRequest**](ReorderOrReplacePlaylistsTracksRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## reorder_or_replace_playlists_tracks

> models::ReorderOrReplacePlaylistsTracks200Response reorder_or_replace_playlists_tracks(playlist_id, uris, reorder_or_replace_playlists_tracks_request)
Update Playlist Items [DEPRECATED] 

**Deprecated:** Use [Update Playlist Items](/documentation/web-api/reference/reorder-or-replace-playlists-items) instead.  Either reorder or replace items in a playlist depending on the request's parameters. To reorder items, include `range_start`, `insert_before`, `range_length` and `snapshot_id` in the request's body. To replace items, include `uris` as either a query parameter or in the request's body. Replacing items in a playlist will overwrite its existing items. This operation can be used for replacing or clearing items in a playlist. <br/> **Note**: Replace and reorder are mutually exclusive operations which share the same endpoint, but have different parameters. These operations can't be applied together in a single request. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**uris** | Option<**String**> |  |  |
**reorder_or_replace_playlists_tracks_request** | Option<[**ReorderOrReplacePlaylistsTracksRequest**](ReorderOrReplacePlaylistsTracksRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## save_tracks_user

> save_tracks_user(save_tracks_user_request)
Save Tracks for Current User 

Save one or more tracks to the current user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**save_tracks_user_request** | Option<[**SaveTracksUserRequest**](SaveTracksUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

